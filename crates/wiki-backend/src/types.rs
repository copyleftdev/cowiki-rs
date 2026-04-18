use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Unique identifier for a wiki page.
/// The relative path from wiki root without extension.
/// E.g., `"ai/transformers"` for `ai/transformers.md`.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageId(pub String);

impl fmt::Display for PageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Metadata for a single wiki page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageMeta {
    pub id: PageId,
    pub path: PathBuf,
    pub title: String,
    pub links_to: Vec<PageId>,
    pub token_cost: u64,
    pub category: u64,
}

/// Serializable mirror of `temporal_graph::TemporalState`.
/// The upstream crate doesn't derive serde, so we convert at the boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTemporalState {
    pub time: u64,
    pub last_access: Vec<u64>,
    pub activation_history: Vec<Vec<f64>>,
    pub health_history: Vec<f64>,
    pub alive: Vec<bool>,
}

impl SerializableTemporalState {
    pub fn to_temporal_state(&self) -> temporal_graph::TemporalState {
        let mut state = temporal_graph::TemporalState::new(self.alive.len());
        state.time = self.time;
        state.last_access = self.last_access.clone();
        state.activation_history = self.activation_history.clone();
        state.health_history = self.health_history.clone();
        state.alive = self.alive.clone();
        state
    }

    pub fn from_temporal_state(state: &temporal_graph::TemporalState) -> Self {
        Self {
            time: state.time,
            last_access: state.last_access.clone(),
            activation_history: state.activation_history.clone(),
            health_history: state.health_history.clone(),
            alive: state.alive.clone(),
        }
    }
}

/// The persistent wiki index.
///
/// Note: raw edge weights live in `ScoredGraph` only — this index used to keep
/// a `Vec<f64>` duplicate for persistence, but that doubled RAM on the dense
/// n² layer. Persistence now pulls weights directly from the graph at save
/// time (see `persist::save`), and load reconstructs a `ScoredGraph` alongside
/// this index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiIndex {
    pub pages: Vec<PageMeta>,
    pub id_to_idx: HashMap<String, usize>,
    pub df: HashMap<String, usize>,
    pub tfidf_vectors: Vec<HashMap<String, f64>>,
    pub temporal_state: SerializableTemporalState,
    pub costs: Vec<u64>,
}

/// Result of a retrieval query.
#[derive(Debug)]
pub struct RetrievalResult {
    pub pages: Vec<PageMeta>,
    pub total_score: f64,
    pub total_cost: u64,
    pub converged: bool,
    pub iterations: usize,
}

/// Errors from wiki-backend operations.
#[derive(Debug)]
pub enum WikiError {
    Io(std::io::Error),
    PageNotFound(PageId),
    SerdeError(String),
}

impl fmt::Display for WikiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WikiError::Io(e) => write!(f, "IO error: {e}"),
            WikiError::PageNotFound(id) => write!(f, "Page not found: {id}"),
            WikiError::SerdeError(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl std::error::Error for WikiError {}

impl From<std::io::Error> for WikiError {
    fn from(e: std::io::Error) -> Self {
        WikiError::Io(e)
    }
}

impl From<rusqlite::Error> for WikiError {
    fn from(e: rusqlite::Error) -> Self {
        WikiError::SerdeError(format!("SQLite error: {e}"))
    }
}
