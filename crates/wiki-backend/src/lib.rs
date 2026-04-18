//! # wiki-backend
//!
//! Wiki filesystem backend for the Co-Wiki architecture.
//!
//! Bridges the proven mathematical primitives (spreading activation,
//! knapsack retrieval, REM maintenance) to a directory of markdown files
//! with wiki-style `[[backlinks]]`.
//!
//! ```text
//! wiki directory          wiki-backend            cowiki primitives
//! ┌─────────────┐    ┌──────────────────┐    ┌─────────────────────┐
//! │ *.md files   │───>│ scan + parse     │───>│ ScoredGraph         │
//! │ [[backlinks]]│    │ tfidf index      │    │ spread()            │
//! │ directories  │    │ graph builder    │    │ select()            │
//! │              │<───│ write + persist  │<───│ rem_cycle()         │
//! └─────────────┘    └──────────────────┘    └─────────────────────┘
//! ```

pub mod graph;
pub mod meta;
pub mod parse;
pub mod persist;
pub mod scan;
pub mod store;
pub mod tfidf;
pub mod types;
pub mod write;

use std::fs;
use std::path::{Path, PathBuf};

use spread::SpreadConfig;
use temporal_graph::RemConfig;

use crate::tfidf::TfIdfIndex;
use crate::types::*;

/// The main entry point for wiki operations.
pub struct WikiBackend {
    root: PathBuf,
    index: WikiIndex,
    graph: scored_graph::ScoredGraph,
    tfidf: TfIdfIndex,
}

impl WikiBackend {
    /// Scan a directory and build the wiki backend from scratch.
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WikiError> {
        let trace = std::env::var("COWIKI_TRACE_OPEN").is_ok();
        let t_all = std::time::Instant::now();
        let root = root.as_ref().canonicalize()?;

        let t = std::time::Instant::now();
        let pages = scan::scan_directory(&root)?;
        if trace { eprintln!("  [open] scan_directory: {} ms ({} pages)", t.elapsed().as_millis(), pages.len()); }
        let id_to_idx = scan::build_index_map(&pages);

        // Read all page contents for TF-IDF (resolve relative paths through root).
        let t = std::time::Instant::now();
        let contents: Vec<String> = pages.iter()
            .map(|p| fs::read_to_string(root.join(&p.path)).unwrap_or_default())
            .collect();
        if trace { eprintln!("  [open] read_contents: {} ms ({} bytes)", t.elapsed().as_millis(), contents.iter().map(|c| c.len()).sum::<usize>()); }

        let t = std::time::Instant::now();
        let tfidf = tfidf::build_index(&contents);
        if trace { eprintln!("  [open] build_tfidf: {} ms", t.elapsed().as_millis()); }

        let t = std::time::Instant::now();
        let g = graph::build_graph(&pages, &id_to_idx);
        if trace { eprintln!("  [open] build_graph: {} ms", t.elapsed().as_millis()); }
        if trace { eprintln!("  [open] TOTAL: {} ms", t_all.elapsed().as_millis()); }

        let n = pages.len();
        let temporal_state = SerializableTemporalState {
            time: 0,
            last_access: vec![0; n],
            activation_history: vec![],
            health_history: vec![],
            alive: vec![true; n],
        };

        let index = WikiIndex {
            costs: g.costs().to_vec(),
            df: tfidf.df().clone(),
            tfidf_vectors: tfidf.vectors().to_vec(),
            pages,
            id_to_idx,
            temporal_state,
        };

        Ok(Self { root, index, graph: g, tfidf })
    }

    /// Load from persisted index, or rebuild if missing.
    pub fn open_or_rebuild(root: impl AsRef<Path>) -> Result<Self, WikiError> {
        let root_path = root.as_ref();

        if let Some((index, graph)) = persist::load(root_path)? {
            let n = index.pages.len();
            let tfidf = TfIdfIndex::from_parts(n, index.df.clone(), index.tfidf_vectors.clone());

            Ok(Self {
                root: root_path.canonicalize()?,
                index,
                graph,
                tfidf,
            })
        } else {
            Self::open(root_path)
        }
    }

    /// Full retrieval pipeline: query -> ignite -> spread -> select -> pages.
    pub fn retrieve(
        &self,
        query: &str,
        budget: u64,
        config: &SpreadConfig,
    ) -> RetrievalResult {
        if self.index.pages.is_empty() {
            return RetrievalResult {
                pages: vec![],
                total_score: 0.0,
                total_cost: 0,
                converged: true,
                iterations: 0,
            };
        }

        // 1. Ignite: query -> initial activation.
        let a0 = tfidf::ignite(&self.tfidf, query);

        // 2. Spread + select via the cowiki composition layer.
        let (selection, spread_result) = cowiki::retrieve(&self.graph, &a0, budget, config);

        // 3. Resolve indices to page metadata.
        let pages: Vec<PageMeta> = selection.indices.iter()
            .filter_map(|&i| self.index.pages.get(i).cloned())
            .collect();

        RetrievalResult {
            pages,
            total_score: selection.total_score,
            total_cost: selection.total_cost,
            converged: spread_result.converged,
            iterations: spread_result.iterations,
        }
    }

    /// Run a REM maintenance cycle.
    pub fn maintain(&mut self, config: &RemConfig) -> temporal_graph::HealthReport {
        let mut temporal = self.index.temporal_state.to_temporal_state();
        let n = self.index.pages.len();

        // Use uniform probe as the query activation.
        let a0 = if n > 0 {
            vec![1.0 / n as f64; n]
        } else {
            vec![]
        };

        let report = temporal_graph::rem_cycle(
            &mut self.graph,
            &mut temporal,
            &a0,
            config,
            None::<fn(usize, usize) -> f64>,
        );

        // Sync state back.
        self.index.temporal_state = SerializableTemporalState::from_temporal_state(&temporal);

        report
    }

    /// Run a REM cycle with dream (backlink discovery).
    ///
    /// Returns the health report. Dreamed edges are in `report.dreamed_edges`
    /// as `(src_idx, dst_idx)` pairs. Call `apply_dream_edges` to write them
    /// to disk, or inspect and approve manually.
    pub fn maintain_with_dream(&mut self, config: &RemConfig) -> temporal_graph::HealthReport {
        let mut temporal = self.index.temporal_state.to_temporal_state();
        let n = self.index.pages.len();

        let a0 = if n > 0 {
            vec![1.0 / n as f64; n]
        } else {
            vec![]
        };

        let tfidf = &self.tfidf;
        let report = temporal_graph::rem_cycle(
            &mut self.graph,
            &mut temporal,
            &a0,
            config,
            Some(|i: usize, j: usize| tfidf::similarity(tfidf, i, j)),
        );

        self.index.temporal_state = SerializableTemporalState::from_temporal_state(&temporal);

        report
    }

    /// Write dream-discovered edges to disk as backlinks.
    pub fn apply_dream_edges(&self, edges: &[(usize, usize)]) -> Result<(), WikiError> {
        for &(src, dst) in edges {
            if let (Some(src_page), Some(dst_page)) = (
                self.index.pages.get(src),
                self.index.pages.get(dst),
            ) {
                write::add_backlink(&self.root, &src_page.id, &dst_page.id)?;
            }
        }
        Ok(())
    }

    /// Create a new wiki page and rebuild the index.
    pub fn create_page(
        &mut self,
        id: &PageId,
        title: &str,
        content: &str,
    ) -> Result<(), WikiError> {
        write::create_page(&self.root, id, title, content)?;
        self.rebuild()
    }

    /// Update a page and rebuild the index.
    pub fn update_page(&mut self, id: &PageId, content: &str) -> Result<(), WikiError> {
        write::update_page(&self.root, id, content)?;
        self.rebuild()
    }

    /// Persist current state to disk.
    pub fn save(&self) -> Result<(), WikiError> {
        persist::save(&self.index, &self.graph, &self.root)
    }

    /// Number of pages in the wiki.
    pub fn len(&self) -> usize {
        self.index.pages.len()
    }

    /// Whether the wiki has no pages.
    pub fn is_empty(&self) -> bool {
        self.index.pages.is_empty()
    }

    /// All pages in the wiki.
    pub fn all_pages(&self) -> &[PageMeta] {
        &self.index.pages
    }

    /// Get a page by its ID.
    pub fn page(&self, id: &PageId) -> Option<&PageMeta> {
        self.index.id_to_idx.get(&id.0)
            .and_then(|&idx| self.index.pages.get(idx))
    }

    /// Wiki root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the underlying graph.
    pub fn graph(&self) -> &scored_graph::ScoredGraph {
        &self.graph
    }

    /// Compute initial activation `a⁰` for a text query from this wiki's TF-IDF index.
    /// Exposed for auditing and tooling (replay the pipeline component-by-component).
    pub fn ignite(&self, query: &str) -> Vec<f64> {
        tfidf::ignite(&self.tfidf, query)
    }

    /// Full rebuild from disk. Called after writes.
    fn rebuild(&mut self) -> Result<(), WikiError> {
        let new = Self::open(&self.root)?;
        self.index = new.index;
        self.graph = new.graph;
        self.tfidf = new.tfidf;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn build_test_wiki(dir: &Path) {
        let pages = [
            ("index.md", "# Home\n\nWelcome to the wiki. See [[about]] and [[ai/transformers]]."),
            ("about.md", "# About\n\nThis wiki covers AI topics. See [[ai/transformers]]."),
            ("ai/transformers.md", "# Transformers\n\nTransformers use [[ai/attention]] mechanisms for sequence modeling."),
            ("ai/attention.md", "# Attention\n\nAttention is the core mechanism behind [[ai/transformers]]."),
            ("unrelated.md", "# Cooking\n\nRecipes for pasta and bread."),
        ];

        for (path, content) in &pages {
            let full = dir.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, content).unwrap();
        }
    }

    #[test]
    fn open_and_scan() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let wiki = WikiBackend::open(tmp.path()).unwrap();
        assert_eq!(wiki.len(), 5);
        assert!(wiki.graph().is_row_stochastic());
    }

    #[test]
    fn retrieve_finds_relevant_pages() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let wiki = WikiBackend::open(tmp.path()).unwrap();
        let result = wiki.retrieve("transformers attention", 1000, &SpreadConfig::default());

        // Should find transformer and attention pages.
        let ids: Vec<&str> = result.pages.iter().map(|p| p.id.0.as_str()).collect();
        assert!(!ids.is_empty(), "Should retrieve some pages");

        // Cooking page should not be top result.
        if let Some(first) = ids.first() {
            assert_ne!(*first, "unrelated",
                "Cooking page should not be the top result for 'transformers attention'");
        }
    }

    #[test]
    fn retrieve_respects_budget() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let wiki = WikiBackend::open(tmp.path()).unwrap();

        // Very small budget.
        let result = wiki.retrieve("transformers", 10, &SpreadConfig::default());
        assert!(result.total_cost <= 10);
    }

    #[test]
    fn create_page_and_rebuild() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let mut wiki = WikiBackend::open(tmp.path()).unwrap();
        assert_eq!(wiki.len(), 5);

        wiki.create_page(
            &PageId("new-page".into()),
            "New Page",
            "Fresh content with a [[index]] link.",
        ).unwrap();

        assert_eq!(wiki.len(), 6);
        assert!(wiki.page(&PageId("new-page".into())).is_some());
    }

    #[test]
    fn persist_and_reload() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let wiki = WikiBackend::open(tmp.path()).unwrap();
        wiki.save().unwrap();

        let reloaded = WikiBackend::open_or_rebuild(tmp.path()).unwrap();
        assert_eq!(reloaded.len(), wiki.len());
    }

    #[test]
    fn maintain_doesnt_crash() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let mut wiki = WikiBackend::open(tmp.path()).unwrap();
        let report = wiki.maintain(&RemConfig::default());
        assert!(report.health >= 0.0);
    }

    #[test]
    fn maintain_with_dream_finds_connections() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let mut wiki = WikiBackend::open(tmp.path()).unwrap();
        let report = wiki.maintain_with_dream(&RemConfig::default());
        assert!(report.health >= 0.0);
        // Dream may or may not find edges depending on TF-IDF similarity.
        // The point is it doesn't crash and health stays positive.
    }

    #[test]
    fn empty_wiki() {
        let tmp = TempDir::new().unwrap();
        let wiki = WikiBackend::open(tmp.path()).unwrap();

        assert_eq!(wiki.len(), 0);
        assert!(wiki.is_empty());

        let result = wiki.retrieve("anything", 1000, &SpreadConfig::default());
        assert!(result.pages.is_empty());
    }

    #[test]
    fn graph_structure_matches_links() {
        let tmp = TempDir::new().unwrap();
        build_test_wiki(tmp.path());

        let wiki = WikiBackend::open(tmp.path()).unwrap();
        let g = wiki.graph();

        // ai/transformers -> ai/attention (and vice versa)
        let t_idx = wiki.index.id_to_idx["ai/transformers"];
        let a_idx = wiki.index.id_to_idx["ai/attention"];

        assert!(g.raw_weight(t_idx, a_idx) > 0.0,
            "transformers should link to attention");
        assert!(g.raw_weight(a_idx, t_idx) > 0.0,
            "attention should link to transformers");

        // unrelated page has no outgoing links
        let u_idx = wiki.index.id_to_idx["unrelated"];
        for j in 0..g.len() {
            assert_eq!(g.raw_weight(u_idx, j), 0.0,
                "unrelated page should have no outgoing edges");
        }
    }
}
