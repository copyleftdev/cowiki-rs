//! Hybrid persistence: companion `.meta` files + SQLite engine database.
//!
//! The human reads the `.meta` files (`cat page.meta`).
//! The engine reads SQLite (`.cowiki/engine.db`).
//!
//! On save: write both. On load: read SQLite (fast).
//! On cold start with no DB: scan `.md` files and rebuild everything.

use std::path::Path;

use crate::meta;
use crate::store;
use crate::tfidf::TfIdfIndex;
use crate::types::*;

/// Save the full wiki state: `.meta` files for humans, SQLite for the engine.
pub fn save(index: &WikiIndex, wiki_root: &Path) -> Result<(), WikiError> {
    // 1. Write companion .meta files (legible layer).
    meta::write_all_meta(&index.pages, wiki_root)?;

    // 2. Write SQLite (engine layer) — single transaction for all writes.
    let mut conn = store::open_db(wiki_root)?;
    let n = index.pages.len();

    let tx = conn.transaction()?;
    store::save_graph(&tx, n, &index.raw_weights, &index.costs)?;
    store::save_tfidf(&tx, &index.df, &index.tfidf_vectors)?;
    store::save_temporal(&tx, &index.temporal_state)?;

    // 3. Save page list as JSON in the meta table (small, structured).
    let pages_json = serde_json::to_string(&index.pages)
        .map_err(|e| WikiError::SerdeError(e.to_string()))?;
    let idx_json = serde_json::to_string(&index.id_to_idx)
        .map_err(|e| WikiError::SerdeError(e.to_string()))?;

    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('pages', ?1)",
        rusqlite::params![pages_json],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('id_to_idx', ?1)",
        rusqlite::params![idx_json],
    )?;
    tx.commit()?;

    Ok(())
}

/// Load from SQLite. Returns `None` if no database exists.
pub fn load(wiki_root: &Path) -> Result<Option<WikiIndex>, WikiError> {
    let db_path = wiki_root.join(".cowiki/engine.db");
    if !db_path.exists() {
        return Ok(None);
    }

    let conn = store::open_db(wiki_root)?;

    // Load graph.
    let (n, raw_weights, costs) = match store::load_graph(&conn)? {
        Some(data) => data,
        None => return Ok(None),
    };

    // Load TF-IDF.
    let (df, tfidf_vectors) = store::load_tfidf(&conn, n)?;

    // Load temporal state.
    let temporal_state = store::load_temporal(&conn)?
        .unwrap_or_else(|| SerializableTemporalState {
            time: 0,
            last_access: vec![0; n],
            activation_history: vec![],
            health_history: vec![],
            alive: vec![true; n],
        });

    // Load page list and index.
    let pages: Vec<PageMeta> = load_meta_value(&conn, "pages")?
        .unwrap_or_default();
    let id_to_idx: std::collections::HashMap<String, usize> = load_meta_value(&conn, "id_to_idx")?
        .unwrap_or_default();

    Ok(Some(WikiIndex {
        pages,
        id_to_idx,
        df,
        tfidf_vectors,
        temporal_state,
        raw_weights,
        costs,
    }))
}

/// Convenience: check if a persisted state exists.
pub fn exists(wiki_root: &Path) -> bool {
    wiki_root.join(".cowiki/engine.db").exists()
}

/// Reconstruct TfIdfIndex from loaded WikiIndex data.
pub fn restore_tfidf(index: &WikiIndex) -> TfIdfIndex {
    TfIdfIndex::from_parts(
        index.pages.len(),
        index.df.clone(),
        index.tfidf_vectors.clone(),
    )
}

fn load_meta_value<T: serde::de::DeserializeOwned>(
    conn: &rusqlite::Connection,
    key: &str,
) -> Result<Option<T>, WikiError> {
    let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
    let result = stmt.query_row(rusqlite::params![key], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    });

    match result {
        Ok(json) => {
            let val: T = serde_json::from_str(&json)
                .map_err(|e| WikiError::SerdeError(e.to_string()))?;
            Ok(Some(val))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageId;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn dummy_index() -> WikiIndex {
        let path = PathBuf::from("/tmp/test.md");
        WikiIndex {
            pages: vec![PageMeta {
                id: PageId("test".into()),
                path: path.clone(),
                title: "Test".into(),
                links_to: vec![PageId("other".into())],
                token_cost: 25,
                category: 0,
            }],
            id_to_idx: [("test".to_string(), 0)].into_iter().collect(),
            df: [("hello".to_string(), 1)].into_iter().collect(),
            tfidf_vectors: vec![[("hello".to_string(), 0.5)].into_iter().collect()],
            temporal_state: SerializableTemporalState {
                time: 5,
                last_access: vec![3],
                activation_history: vec![vec![0.7]],
                health_history: vec![0.9],
                alive: vec![true],
            },
            raw_weights: vec![0.0],
            costs: vec![25],
        }
    }

    #[test]
    fn hybrid_round_trip() {
        let tmp = TempDir::new().unwrap();
        // Create the .md file so .meta can be written alongside it.
        std::fs::write(tmp.path().join("test.md"), "# Test").unwrap();

        let mut index = dummy_index();
        index.pages[0].path = tmp.path().join("test.md");

        save(&index, tmp.path()).unwrap();

        // Meta file should exist.
        assert!(tmp.path().join("test.meta").exists(), "Meta file not written");
        let meta_content = std::fs::read_to_string(tmp.path().join("test.meta")).unwrap();
        assert!(meta_content.contains("title: Test"), "Meta should contain title");
        assert!(meta_content.contains("- other"), "Meta should contain backlink");

        // SQLite should exist.
        assert!(tmp.path().join(".cowiki/engine.db").exists(), "DB not written");

        // Load back.
        let loaded = load(tmp.path()).unwrap().unwrap();
        assert_eq!(loaded.pages.len(), 1);
        assert_eq!(loaded.pages[0].title, "Test");
        assert_eq!(loaded.temporal_state.time, 5);
        assert_eq!(loaded.df["hello"], 1);
        assert_eq!(loaded.costs, vec![25]);
    }

    #[test]
    fn load_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert!(load(tmp.path()).unwrap().is_none());
    }

    #[test]
    fn exists_check() {
        let tmp = TempDir::new().unwrap();
        assert!(!exists(tmp.path()));

        let mut index = dummy_index();
        index.pages[0].path = PathBuf::from("/dev/null"); // meta write will fail but DB won't
        // Just create the DB directly.
        store::open_db(tmp.path()).unwrap();
        // exists checks for the file, and open_db creates it.
        assert!(exists(tmp.path()));
    }
}
