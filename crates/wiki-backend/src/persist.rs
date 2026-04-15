use std::fs;
use std::path::Path;

use crate::types::{WikiError, WikiIndex};

const META_DIR: &str = ".cowiki";
const INDEX_FILE: &str = "index.json";

/// Save the wiki index to `.cowiki/index.json` (atomic write).
pub fn save(index: &WikiIndex, wiki_root: &Path) -> Result<(), WikiError> {
    let meta_dir = wiki_root.join(META_DIR);
    fs::create_dir_all(&meta_dir)?;

    let json = serde_json::to_string_pretty(index)
        .map_err(|e| WikiError::SerdeError(e.to_string()))?;

    // Atomic write: write to tmp, then rename.
    let tmp_path = meta_dir.join(format!("{INDEX_FILE}.tmp"));
    let final_path = meta_dir.join(INDEX_FILE);

    fs::write(&tmp_path, json)?;
    fs::rename(&tmp_path, &final_path)?;

    Ok(())
}

/// Load the wiki index from `.cowiki/index.json`.
/// Returns `Ok(None)` if no index file exists.
pub fn load(wiki_root: &Path) -> Result<Option<WikiIndex>, WikiError> {
    let path = wiki_root.join(META_DIR).join(INDEX_FILE);

    if !path.exists() {
        return Ok(None);
    }

    let json = fs::read_to_string(&path)?;
    let index: WikiIndex = serde_json::from_str(&json)
        .map_err(|e| WikiError::SerdeError(e.to_string()))?;

    Ok(Some(index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PageId, PageMeta, SerializableTemporalState};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn dummy_index() -> WikiIndex {
        WikiIndex {
            pages: vec![PageMeta {
                id: PageId("test".into()),
                path: PathBuf::from("test.md"),
                title: "Test".into(),
                links_to: vec![],
                token_cost: 25,
                category: 0,
            }],
            id_to_idx: [("test".to_string(), 0)].into_iter().collect(),
            df: HashMap::new(),
            tfidf_vectors: vec![HashMap::new()],
            temporal_state: SerializableTemporalState {
                time: 0,
                last_access: vec![0],
                activation_history: vec![],
                health_history: vec![],
                alive: vec![true],
            },
            raw_weights: vec![0.0],
            costs: vec![25],
        }
    }

    #[test]
    fn round_trip() {
        let tmp = TempDir::new().unwrap();
        let index = dummy_index();

        save(&index, tmp.path()).unwrap();

        let loaded = load(tmp.path()).unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.pages.len(), 1);
        assert_eq!(loaded.pages[0].title, "Test");
    }

    #[test]
    fn load_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        let loaded = load(tmp.path()).unwrap();
        assert!(loaded.is_none());
    }
}
