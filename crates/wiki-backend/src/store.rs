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

/// (n, weights, costs) tuple returned from graph loading.
pub type GraphData = (usize, Vec<f64>, Vec<u64>);

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

/// Save the weight matrix and costs.
pub fn save_graph(
    conn: &Connection,
    n: usize,
    weights: &[f64],
    costs: &[u64],
) -> Result<(), WikiError> {
    let weights_blob = f64_slice_to_bytes(weights);
    let costs_blob = u64_slice_to_bytes(costs);

    conn.execute(
        "INSERT OR REPLACE INTO graph (id, n, weights, costs) VALUES (1, ?1, ?2, ?3)",
        params![n as i64, weights_blob, costs_blob],
    )?;
    Ok(())
}

/// Load the weight matrix and costs.
pub fn load_graph(conn: &Connection) -> Result<Option<GraphData>, WikiError> {
    let mut stmt = conn.prepare("SELECT n, weights, costs FROM graph WHERE id = 1")?;
    let result = stmt.query_row([], |row| {
        let n: i64 = row.get(0)?;
        let weights_blob: Vec<u8> = row.get(1)?;
        let costs_blob: Vec<u8> = row.get(2)?;
        Ok((n as usize, weights_blob, costs_blob))
    });

    match result {
        Ok((n, wb, cb)) => {
            let weights = bytes_to_f64_vec(&wb);
            let costs = bytes_to_u64_vec(&cb);
            Ok(Some((n, weights, costs)))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
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

fn u64_slice_to_bytes(data: &[u64]) -> Vec<u8> {
    data.iter().flat_map(|u| u.to_le_bytes()).collect()
}

fn bytes_to_u64_vec(bytes: &[u8]) -> Vec<u64> {
    bytes.chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
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

        let weights = vec![0.0, 1.0, 0.5, 0.0];
        let costs = vec![100, 200];

        save_graph(&conn, 2, &weights, &costs).unwrap();

        let (n, loaded_w, loaded_c) = load_graph(&conn).unwrap().unwrap();
        assert_eq!(n, 2);
        assert_eq!(loaded_w, weights);
        assert_eq!(loaded_c, costs);
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

        assert!(load_graph(&conn).unwrap().is_none());
        assert!(load_temporal(&conn).unwrap().is_none());
    }

    #[test]
    fn overwrite_preserves_latest() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        save_graph(&conn, 1, &[0.0], &[100]).unwrap();
        save_graph(&conn, 2, &[0.0, 1.0, 0.5, 0.0], &[100, 200]).unwrap();

        let (n, _, _) = load_graph(&conn).unwrap().unwrap();
        assert_eq!(n, 2);
    }
}
