//! SQLite storage for computational state.
//!
//! Stores the expensive, non-legible data that doesn't belong in text files:
//! - Weight matrix (n² floats as a blob)
//! - TF-IDF document frequencies and vectors
//! - Temporal state (access times, activation history, health)
//! - Node costs
//!
//! The `.meta` files are the legible layer for humans.
//! This is the engine layer for the machine.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};

use crate::types::{SerializableTemporalState, WikiError};

/// Forward CSR graph data as persisted on disk.
pub struct CsrGraphData {
    pub n: usize,
    pub row_ptr: Vec<usize>,
    pub col_idx: Vec<usize>,
    pub values: Vec<f32>,
    pub costs: Vec<u64>,
}

/// (document_frequencies, tfidf_vectors) tuple returned from TF-IDF loading.
pub type TfIdfData = (HashMap<String, usize>, Vec<HashMap<String, f64>>);

const DB_NAME: &str = "engine.db";

/// Open (or create) the engine database at `.cowiki/engine.db`.
pub fn open_db(wiki_root: &Path) -> Result<Connection, WikiError> {
    let meta_dir = wiki_root.join(".cowiki");
    std::fs::create_dir_all(&meta_dir)?;
    let db_path = meta_dir.join(DB_NAME);
    let conn = Connection::open(db_path)?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), WikiError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS graph (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            n INTEGER NOT NULL,
            weights BLOB NOT NULL,
            costs BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tfidf_df (
            term TEXT PRIMARY KEY,
            count INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tfidf_vectors (
            doc_idx INTEGER NOT NULL,
            term TEXT NOT NULL,
            value REAL NOT NULL,
            PRIMARY KEY (doc_idx, term)
        );

        CREATE TABLE IF NOT EXISTS temporal (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            time INTEGER NOT NULL,
            last_access BLOB NOT NULL,
            health_history BLOB NOT NULL,
            alive BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS activation_history (
            step INTEGER PRIMARY KEY,
            activation BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );"
    )?;
    Ok(())
}

/// Save the graph as three CSR sidecar files in `.cowiki/` plus a tiny
/// SQLite row holding `n` and `costs`. Write cost is O(nnz), not O(n²) —
/// at n=25k that's ~2 MB instead of the 5 GB dense blob that hit
/// SQLite's per-row size ceiling.
///
/// The sidecar layout is chosen to be mmap-friendly: each file is a
/// plain binary array of a single fixed-width type, so future code can
/// `memmap2::Mmap` them and `bytemuck::cast_slice` to `&[usize]` /
/// `&[f32]` without any parsing.
pub fn save_graph(
    conn: &Connection,
    wiki_root: &Path,
    n: usize,
    row_ptr: &[usize],
    col_idx: &[usize],
    values: &[f32],
    costs: &[u64],
) -> Result<(), WikiError> {
    let meta_dir = wiki_root.join(".cowiki");
    std::fs::create_dir_all(&meta_dir)?;
    std::fs::write(meta_dir.join("graph.row_ptr"), usize_slice_to_bytes(row_ptr))?;
    std::fs::write(meta_dir.join("graph.col_idx"), usize_slice_to_bytes(col_idx))?;
    std::fs::write(meta_dir.join("graph.values"),  f32_slice_to_bytes(values))?;

    // Tiny SQLite row holds only `n` and `costs` (small) plus a zero-length
    // placeholder in the legacy `weights` blob column so the NOT NULL
    // constraint on older schemas is still satisfied.
    let costs_blob = u64_slice_to_bytes(costs);
    conn.execute(
        "INSERT OR REPLACE INTO graph (id, n, weights, costs) VALUES (1, ?1, ?2, ?3)",
        params![n as i64, Vec::<u8>::new(), costs_blob],
    )?;
    Ok(())
}

/// Load graph state from the CSR sidecars + SQLite costs row. Returns
/// `None` if either the SQLite row or any of the three sidecar files is
/// missing — the caller falls back to a full rescan from markdown.
pub fn load_graph(conn: &Connection, wiki_root: &Path) -> Result<Option<CsrGraphData>, WikiError> {
    // SQLite: read n and costs.
    let mut stmt = conn.prepare("SELECT n, costs FROM graph WHERE id = 1")?;
    let result = stmt.query_row([], |row| {
        let n: i64 = row.get(0)?;
        let costs_blob: Vec<u8> = row.get(1)?;
        Ok((n as usize, costs_blob))
    });
    let (n, costs_blob) = match result {
        Ok(data) => data,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let costs = bytes_to_u64_vec(&costs_blob);
    if costs.len() != n {
        return Ok(None); // inconsistent row — force rebuild
    }

    // Sidecars.
    let meta_dir = wiki_root.join(".cowiki");
    let rp_path = meta_dir.join("graph.row_ptr");
    let ci_path = meta_dir.join("graph.col_idx");
    let v_path  = meta_dir.join("graph.values");
    if !rp_path.exists() || !ci_path.exists() || !v_path.exists() {
        return Ok(None); // sidecars missing — caller will rebuild
    }

    let row_ptr = bytes_to_usize_vec(&std::fs::read(&rp_path)?);
    let col_idx = bytes_to_usize_vec(&std::fs::read(&ci_path)?);
    let values  = bytes_to_f32_vec(&std::fs::read(&v_path)?);

    // Light sanity — the from_raw_csr constructor will do the thorough pass.
    if row_ptr.len() != n + 1 || col_idx.len() != values.len() {
        return Ok(None);
    }

    Ok(Some(CsrGraphData { n, row_ptr, col_idx, values, costs }))
}

/// Save TF-IDF document frequencies and vectors.
pub fn save_tfidf(
    conn: &Connection,
    df: &HashMap<String, usize>,
    vectors: &[HashMap<String, f64>],
) -> Result<(), WikiError> {
    conn.execute("DELETE FROM tfidf_df", [])?;
    conn.execute("DELETE FROM tfidf_vectors", [])?;

    let mut df_stmt = conn.prepare("INSERT INTO tfidf_df (term, count) VALUES (?1, ?2)")?;
    for (term, count) in df {
        df_stmt.execute(params![term, *count as i64])?;
    }

    let mut vec_stmt = conn.prepare(
        "INSERT INTO tfidf_vectors (doc_idx, term, value) VALUES (?1, ?2, ?3)"
    )?;
    for (idx, doc) in vectors.iter().enumerate() {
        for (term, value) in doc {
            vec_stmt.execute(params![idx as i64, term, value])?;
        }
    }

    Ok(())
}

/// Load TF-IDF document frequencies and vectors.
pub fn load_tfidf(
    conn: &Connection,
    n_docs: usize,
) -> Result<TfIdfData, WikiError> {
    let mut df = HashMap::new();
    let mut stmt = conn.prepare("SELECT term, count FROM tfidf_df")?;
    let rows = stmt.query_map([], |row| {
        let term: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((term, count as usize))
    })?;
    for row in rows {
        let (term, count) = row?;
        df.insert(term, count);
    }

    let mut vectors: Vec<HashMap<String, f64>> = vec![HashMap::new(); n_docs];
    let mut stmt = conn.prepare("SELECT doc_idx, term, value FROM tfidf_vectors")?;
    let rows = stmt.query_map([], |row| {
        let idx: i64 = row.get(0)?;
        let term: String = row.get(1)?;
        let value: f64 = row.get(2)?;
        Ok((idx as usize, term, value))
    })?;
    for row in rows {
        let (idx, term, value) = row?;
        if idx < n_docs {
            vectors[idx].insert(term, value);
        }
    }

    Ok((df, vectors))
}

/// Save temporal state.
pub fn save_temporal(
    conn: &Connection,
    state: &SerializableTemporalState,
) -> Result<(), WikiError> {
    let last_access_blob = u64_slice_to_bytes(&state.last_access);
    let health_blob = f64_slice_to_bytes(&state.health_history);
    let alive_blob: Vec<u8> = state.alive.iter().map(|&b| if b { 1u8 } else { 0u8 }).collect();

    conn.execute(
        "INSERT OR REPLACE INTO temporal (id, time, last_access, health_history, alive) VALUES (1, ?1, ?2, ?3, ?4)",
        params![state.time as i64, last_access_blob, health_blob, alive_blob],
    )?;

    // Save activation history.
    conn.execute("DELETE FROM activation_history", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO activation_history (step, activation) VALUES (?1, ?2)"
    )?;
    for (step, activation) in state.activation_history.iter().enumerate() {
        let blob = f64_slice_to_bytes(activation);
        stmt.execute(params![step as i64, blob])?;
    }

    Ok(())
}

/// Load temporal state.
pub fn load_temporal(conn: &Connection) -> Result<Option<SerializableTemporalState>, WikiError> {
    let mut stmt = conn.prepare(
        "SELECT time, last_access, health_history, alive FROM temporal WHERE id = 1"
    )?;

    let result = stmt.query_row([], |row| {
        let time: i64 = row.get(0)?;
        let last_access_blob: Vec<u8> = row.get(1)?;
        let health_blob: Vec<u8> = row.get(2)?;
        let alive_blob: Vec<u8> = row.get(3)?;
        Ok((time as u64, last_access_blob, health_blob, alive_blob))
    });

    let (time, la_blob, h_blob, a_blob) = match result {
        Ok(data) => data,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let last_access = bytes_to_u64_vec(&la_blob);
    let health_history = bytes_to_f64_vec(&h_blob);
    let alive: Vec<bool> = a_blob.iter().map(|&b| b != 0).collect();

    // Load activation history.
    let mut activation_history = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT activation FROM activation_history ORDER BY step"
    )?;
    let rows = stmt.query_map([], |row| {
        let blob: Vec<u8> = row.get(0)?;
        Ok(blob)
    })?;
    for row in rows {
        activation_history.push(bytes_to_f64_vec(&row?));
    }

    Ok(Some(SerializableTemporalState {
        time,
        last_access,
        activation_history,
        health_history,
        alive,
    }))
}

// ─── Byte conversion helpers ─────────────────────────────────────────────────

fn f64_slice_to_bytes(data: &[f64]) -> Vec<u8> {
    data.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_f64_vec(bytes: &[u8]) -> Vec<f64> {
    bytes.chunks_exact(8)
        .map(|chunk| f64::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn f32_slice_to_bytes(data: &[f32]) -> Vec<u8> {
    data.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn u64_slice_to_bytes(data: &[u64]) -> Vec<u8> {
    data.iter().flat_map(|u| u.to_le_bytes()).collect()
}

fn bytes_to_u64_vec(bytes: &[u8]) -> Vec<u64> {
    bytes.chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn usize_slice_to_bytes(data: &[usize]) -> Vec<u8> {
    // Persist as u64 LE so .cowiki sidecars are portable across 32/64-bit.
    data.iter().flat_map(|u| (*u as u64).to_le_bytes()).collect()
}

fn bytes_to_usize_vec(bytes: &[u8]) -> Vec<usize> {
    bytes.chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()) as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn graph_round_trip() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        // 2-node graph: 0 -> 1 with weight 1.0. CSR: row_ptr=[0,1,1],
        // col_idx=[1], values=[1.0].
        let row_ptr: Vec<usize> = vec![0, 1, 1];
        let col_idx: Vec<usize> = vec![1];
        let values:  Vec<f32>   = vec![1.0];
        let costs = vec![100u64, 200];

        save_graph(&conn, tmp.path(), 2, &row_ptr, &col_idx, &values, &costs).unwrap();

        let loaded = load_graph(&conn, tmp.path()).unwrap().unwrap();
        assert_eq!(loaded.n, 2);
        assert_eq!(loaded.row_ptr, row_ptr);
        assert_eq!(loaded.col_idx, col_idx);
        assert_eq!(loaded.values, values);
        assert_eq!(loaded.costs, costs);
    }

    #[test]
    fn tfidf_round_trip() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        let mut df = HashMap::new();
        df.insert("hello".into(), 3);
        df.insert("world".into(), 2);

        let mut v0 = HashMap::new();
        v0.insert("hello".into(), 0.5);
        let v1 = HashMap::new();
        let vectors = vec![v0, v1];

        save_tfidf(&conn, &df, &vectors).unwrap();

        let (loaded_df, loaded_vecs) = load_tfidf(&conn, 2).unwrap();
        assert_eq!(loaded_df["hello"], 3);
        assert_eq!(loaded_df["world"], 2);
        assert_eq!(loaded_vecs[0]["hello"], 0.5);
        assert!(loaded_vecs[1].is_empty());
    }

    #[test]
    fn temporal_round_trip() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        let state = SerializableTemporalState {
            time: 42,
            last_access: vec![10, 20, 30],
            activation_history: vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]],
            health_history: vec![0.9, 0.85],
            alive: vec![true, false, true],
        };

        save_temporal(&conn, &state).unwrap();

        let loaded = load_temporal(&conn).unwrap().unwrap();
        assert_eq!(loaded.time, 42);
        assert_eq!(loaded.last_access, vec![10, 20, 30]);
        assert_eq!(loaded.activation_history.len(), 2);
        assert_eq!(loaded.health_history, vec![0.9, 0.85]);
        assert_eq!(loaded.alive, vec![true, false, true]);
    }

    #[test]
    fn load_empty_db() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        assert!(load_graph(&conn, tmp.path()).unwrap().is_none());
        assert!(load_temporal(&conn).unwrap().is_none());
    }

    #[test]
    fn overwrite_preserves_latest() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        save_graph(&conn, tmp.path(), 1, &[0, 0], &[], &[], &[100]).unwrap();
        save_graph(&conn, tmp.path(), 2, &[0, 1, 1], &[1], &[1.0], &[100, 200]).unwrap();

        let loaded = load_graph(&conn, tmp.path()).unwrap().unwrap();
        let n = loaded.n;
        assert_eq!(n, 2);
    }
}
