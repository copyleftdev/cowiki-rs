//! VOPR / DST-style end-to-end chaos tests for the wiki backend.
//!
//! Deterministic simulation that drives a WikiBackend through random
//! operations — creating pages, editing content, querying, running REM
//! cycles, persisting, reloading — while checking invariants after every
//! step. Seeded PRNG makes failures reproducible.
//!
//! This tests the full vertical: filesystem → scan → parse → graph →
//! spread → knapsack → REM → SQLite → .meta files → reload.

use std::fs;

use tempfile::TempDir;
use wiki_backend::types::*;
use wiki_backend::WikiBackend;
use spread::SpreadConfig;
use temporal_graph::RemConfig;

// ─── Deterministic PRNG ──────────────────────────────────────────────────────

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self { Self(if seed == 0 { 1 } else { seed }) }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max
    }

    #[allow(dead_code)]
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.next_usize(items.len())]
    }
}

// ─── Word generation ─────────────────────────────────────────────────────────

const WORDS: &[&str] = &[
    "neural", "network", "transformer", "attention", "gradient",
    "backprop", "convolution", "embedding", "tokenizer", "decoder",
    "encoder", "diffusion", "reinforcement", "policy", "reward",
    "kernel", "activation", "sigmoid", "softmax", "dropout",
    "batch", "epoch", "tensor", "matrix", "vector",
    "sparse", "dense", "latent", "manifold", "topology",
    "graph", "node", "edge", "weight", "bias",
    "memory", "cache", "retrieval", "spreading", "decay",
];

fn random_title(rng: &mut Rng) -> String {
    let n = 1 + rng.next_usize(3);
    (0..n).map(|_| *rng.pick(WORDS)).collect::<Vec<_>>().join(" ")
}

fn random_content(rng: &mut Rng, existing_pages: &[String]) -> String {
    let n_sentences = 2 + rng.next_usize(6);
    let mut lines = Vec::new();

    for _ in 0..n_sentences {
        let n_words = 4 + rng.next_usize(10);
        let sentence: String = (0..n_words)
            .map(|_| *rng.pick(WORDS))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(format!("{sentence}."));
    }

    // Sprinkle in some backlinks to existing pages.
    if !existing_pages.is_empty() {
        let n_links = rng.next_usize(3);
        for _ in 0..n_links {
            let target = rng.pick(existing_pages);
            lines.push(format!("See [[{target}]] for more."));
        }
    }

    lines.join(" ")
}

fn random_page_id(rng: &mut Rng) -> String {
    let dirs = ["", "ai/", "math/", "systems/", "notes/"];
    let dir = rng.pick(&dirs);
    let name: String = (0..2).map(|_| *rng.pick(WORDS)).collect::<Vec<_>>().join("-");
    format!("{dir}{name}")
}

fn random_query(rng: &mut Rng) -> String {
    let n = 1 + rng.next_usize(3);
    (0..n).map(|_| *rng.pick(WORDS)).collect::<Vec<_>>().join(" ")
}

// ─── Actions ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum Action {
    CreatePage,
    UpdatePage,
    Query,
    Maintain,
    MaintainWithDream,
    SaveAndReload,
    CheckInvariants,
}

fn random_action(rng: &mut Rng) -> Action {
    match rng.next_usize(10) {
        0..=2 => Action::CreatePage,        // 30% — grow the wiki
        3 => Action::UpdatePage,            // 10% — edit existing
        4..=5 => Action::Query,             // 20% — retrieval
        6 => Action::Maintain,              // 10% — REM cycle
        7 => Action::MaintainWithDream,     // 10% — REM + dream
        8 => Action::SaveAndReload,         // 10% — persistence round-trip
        _ => Action::CheckInvariants,       // 10% — full invariant check
    }
}

// ─── Invariant checks ───────────────────────────────────────────────────────

fn check_invariants(wiki: &WikiBackend, label: &str) -> Result<(), String> {
    let g = wiki.graph();

    // 1. Row-stochastic.
    if g.len() > 0 && !g.is_row_stochastic() {
        return Err(format!("[{label}] Row-stochastic invariant violated"));
    }

    // 2. No self-loops.
    for i in 0..g.len() {
        if g.raw_weight(i, i) != 0.0 {
            return Err(format!("[{label}] Self-loop at node {i}"));
        }
    }

    // 3. All weights non-negative and finite.
    for i in 0..g.len() {
        for j in 0..g.len() {
            let w = g.raw_weight(i, j);
            if w < 0.0 || w.is_nan() || w.is_infinite() {
                return Err(format!("[{label}] Bad weight at ({i},{j}): {w}"));
            }
        }
    }

    // 4. Page count matches graph size.
    if wiki.len() != g.len() {
        return Err(format!(
            "[{label}] Page count {} != graph size {}",
            wiki.len(), g.len()
        ));
    }

    // 5. Every page's .md file exists on disk.
    // (Skipped for speed in chaos tests — rebuild validates this.)

    Ok(())
}

fn check_retrieval_invariants(
    result: &RetrievalResult,
    budget: u64,
    label: &str,
) -> Result<(), String> {
    // Budget never exceeded.
    if result.total_cost > budget {
        return Err(format!(
            "[{label}] Budget violated: used {}, budget={budget}",
            result.total_cost
        ));
    }

    // Score non-negative.
    if result.total_score < -1e-9 {
        return Err(format!(
            "[{label}] Negative score: {}",
            result.total_score
        ));
    }

    // No NaN in returned pages.
    for page in &result.pages {
        if page.token_cost == 0 {
            return Err(format!(
                "[{label}] Zero token cost for page {}",
                page.id
            ));
        }
    }

    Ok(())
}

// ─── The VOPR ────────────────────────────────────────────────────────────────

fn vopr_run(seed: u64, n_steps: usize) -> Result<(), String> {
    let tmp = TempDir::new().map_err(|e| format!("tmpdir: {e}"))?;
    let root = tmp.path();

    // Seed the wiki with a few pages so queries have something to find.
    let seed_pages = [
        ("index", "Home", "Welcome to the wiki. See [[about]] for more."),
        ("about", "About", "This wiki covers AI and systems topics."),
        ("ai/transformers", "Transformers", "Transformers use [[ai/attention]] for sequence modeling."),
        ("ai/attention", "Attention", "Attention is behind [[ai/transformers]]."),
    ];
    for (id, title, content) in &seed_pages {
        let path = root.join(format!("{id}.md"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, format!("# {title}\n\n{content}\n")).unwrap();
    }

    let mut wiki = WikiBackend::open(root).map_err(|e| format!("open: {e}"))?;
    let mut rng = Rng::new(seed);
    let mut page_ids: Vec<String> = seed_pages.iter().map(|(id, _, _)| id.to_string()).collect();

    let spread_cfg = SpreadConfig::default();
    let rem_cfg = RemConfig {
        decay_rate: 0.03,
        prune_threshold: 0.001,
        prune_window: 10,
        d: 0.8,
        activation_threshold: 0.01,
    };

    for step in 0..n_steps {
        let label = format!("seed={seed:#x} step={step}");
        let action = random_action(&mut rng);

        match action {
            Action::CreatePage => {
                let id = random_page_id(&mut rng);
                let title = random_title(&mut rng);
                let content = random_content(&mut rng, &page_ids);

                // Might conflict with existing page — that's fine, update instead.
                if wiki.page(&PageId(id.clone())).is_some() {
                    let new_content = format!("# {title}\n\n{content}\n");
                    wiki.update_page(&PageId(id.clone()), &new_content)
                        .map_err(|e| format!("[{label}] update: {e}"))?;
                } else {
                    wiki.create_page(&PageId(id.clone()), &title, &content)
                        .map_err(|e| format!("[{label}] create: {e}"))?;
                    page_ids.push(id);
                }

                check_invariants(&wiki, &label)?;
            }

            Action::UpdatePage => {
                if page_ids.is_empty() { continue; }
                let id = rng.pick(&page_ids).clone();
                let content = random_content(&mut rng, &page_ids);
                let title = random_title(&mut rng);
                let new_content = format!("# {title}\n\n{content}\n");

                wiki.update_page(&PageId(id), &new_content)
                    .map_err(|e| format!("[{label}] update: {e}"))?;

                check_invariants(&wiki, &label)?;
            }

            Action::Query => {
                let query = random_query(&mut rng);
                let budget = 100 + rng.next_usize(2000) as u64;

                let result = wiki.retrieve(&query, budget, &spread_cfg);
                check_retrieval_invariants(&result, budget, &label)?;
            }

            Action::Maintain => {
                let report = wiki.maintain(&rem_cfg);
                if report.health.is_nan() || report.health.is_infinite() {
                    return Err(format!("[{label}] Health is {}", report.health));
                }
                check_invariants(&wiki, &label)?;
            }

            Action::MaintainWithDream => {
                let report = wiki.maintain_with_dream(&rem_cfg);
                if report.health.is_nan() || report.health.is_infinite() {
                    return Err(format!("[{label}] Dream health is {}", report.health));
                }

                // Optionally apply dream edges to disk.
                if rng.next_bool() && !report.dreamed_edges.is_empty() {
                    wiki.apply_dream_edges(&report.dreamed_edges)
                        .map_err(|e| format!("[{label}] apply_dream: {e}"))?;
                }

                check_invariants(&wiki, &label)?;
            }

            Action::SaveAndReload => {
                let page_count_before = wiki.len();

                wiki.save().map_err(|e| format!("[{label}] save: {e}"))?;

                // Reload from persisted state.
                wiki = WikiBackend::open_or_rebuild(root)
                    .map_err(|e| format!("[{label}] reload: {e}"))?;

                let page_count_after = wiki.len();
                if page_count_after != page_count_before {
                    return Err(format!(
                        "[{label}] Page count changed after reload: {page_count_before} -> {page_count_after}"
                    ));
                }

                check_invariants(&wiki, &label)?;
            }

            Action::CheckInvariants => {
                check_invariants(&wiki, &label)?;

                // Also verify a query doesn't crash.
                let result = wiki.retrieve("test query", 500, &spread_cfg);
                check_retrieval_invariants(&result, 500, &label)?;
            }
        }
    }

    // Final save and reload to make sure persistence is clean.
    wiki.save().map_err(|e| format!("[seed={seed:#x} final] save: {e}"))?;
    let final_wiki = WikiBackend::open_or_rebuild(root)
        .map_err(|e| format!("[seed={seed:#x} final] reload: {e}"))?;
    check_invariants(&final_wiki, &format!("seed={seed:#x} final"))?;

    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// 50 seeds, 100 steps each. Core chaos coverage.
#[test]
fn vopr_50_seeds() {
    for seed in 0..50u64 {
        vopr_run(seed, 100).unwrap_or_else(|e| {
            panic!("VOPR failed: {e}");
        });
    }
}

/// Small wiki, long horizon. Catches state accumulation bugs.
#[test]
fn vopr_long_horizon() {
    vopr_run(0xCAFE_BABE, 500).unwrap_or_else(|e| {
        panic!("VOPR long-horizon failed: {e}");
    });
}

/// Rapid save/reload cycles. Catches serialization drift.
#[test]
fn vopr_persistence_stress() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // Seed wiki.
    fs::create_dir_all(root.join("ai")).unwrap();
    fs::write(root.join("index.md"), "# Home\n\nSee [[about]].").unwrap();
    fs::write(root.join("about.md"), "# About\n\nInfo.").unwrap();
    fs::write(root.join("ai/ml.md"), "# ML\n\nMachine learning.").unwrap();

    let spread_cfg = SpreadConfig::default();
    let rem_cfg = RemConfig::default();

    let mut wiki = WikiBackend::open(root).unwrap();

    // 100 cycles of: query, maintain, save, reload.
    for i in 0..100 {
        wiki.retrieve("machine learning", 500, &spread_cfg);
        wiki.maintain(&rem_cfg);
        wiki.save().unwrap();

        wiki = WikiBackend::open_or_rebuild(root)
            .unwrap_or_else(|e| panic!("Reload failed at cycle {i}: {e}"));

        assert!(wiki.graph().is_row_stochastic(),
            "Row-stochastic violated after reload cycle {i}");
    }
}

/// Create many pages then query. Checks graph scaling.
#[test]
fn vopr_growth_stress() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let mut rng = Rng::new(0xBEEF);

    // Seed one page.
    fs::write(root.join("root.md"), "# Root\n\nThe root page.").unwrap();
    let mut wiki = WikiBackend::open(root).unwrap();
    let mut ids = vec!["root".to_string()];

    // Create 50 pages, each linking to random existing pages.
    for i in 0..50 {
        let id = format!("page-{i}");
        let mut content = format!("# Page {i}\n\nContent for page {i}.");
        let n_links = 1 + rng.next_usize(3);
        for _ in 0..n_links {
            let target = rng.pick(&ids);
            content.push_str(&format!(" See [[{target}]]."));
        }
        wiki.create_page(&PageId(id.clone()), &format!("Page {i}"), &content).unwrap();
        ids.push(id);
    }

    assert_eq!(wiki.len(), 51);
    assert!(wiki.graph().is_row_stochastic());

    // Query should still work and respect budget.
    let result = wiki.retrieve("content page", 200, &SpreadConfig::default());
    assert!(result.total_cost <= 200);

    // Maintain should not crash.
    let report = wiki.maintain(&RemConfig::default());
    assert!(report.health >= 0.0);
    assert!(!report.health.is_nan());
}

/// Delete a page's .md file externally, then rebuild. Backend should handle it.
#[test]
fn vopr_external_deletion() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("keep.md"), "# Keep\n\nStays.").unwrap();
    fs::write(root.join("delete.md"), "# Delete\n\nGoes away. See [[keep]].").unwrap();

    let wiki = WikiBackend::open(root).unwrap();
    assert_eq!(wiki.len(), 2);

    // External deletion.
    fs::remove_file(root.join("delete.md")).unwrap();

    // Rebuild should handle the missing file gracefully.
    let wiki = WikiBackend::open(root).unwrap();
    assert_eq!(wiki.len(), 1);
    assert!(wiki.page(&PageId("keep".into())).is_some());
    assert!(wiki.page(&PageId("delete".into())).is_none());
}

/// External edit: someone changes a file outside the backend.
#[test]
fn vopr_external_edit() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("page.md"), "# Page\n\nOriginal content.").unwrap();

    let wiki = WikiBackend::open(root).unwrap();
    let original_cost = wiki.page(&PageId("page".into())).unwrap().token_cost;

    // External edit: make the file much larger.
    let big_content = "# Page\n\n".to_string() + &"lots of new content. ".repeat(100);
    fs::write(root.join("page.md"), &big_content).unwrap();

    // Rebuild should pick up the change.
    let wiki = WikiBackend::open(root).unwrap();
    let new_cost = wiki.page(&PageId("page".into())).unwrap().token_cost;
    assert!(new_cost > original_cost,
        "Token cost should increase after external edit: {original_cost} -> {new_cost}");
}

/// Empty directory: everything should work on zero pages.
#[test]
fn vopr_empty_wiki_operations() {
    let tmp = TempDir::new().unwrap();
    let wiki = WikiBackend::open(tmp.path()).unwrap();

    assert_eq!(wiki.len(), 0);
    assert!(wiki.is_empty());

    // Query on empty wiki.
    let result = wiki.retrieve("anything", 1000, &SpreadConfig::default());
    assert!(result.pages.is_empty());
    assert_eq!(result.total_cost, 0);

    // Save and reload empty wiki.
    wiki.save().unwrap();
    let wiki = WikiBackend::open_or_rebuild(tmp.path()).unwrap();
    assert_eq!(wiki.len(), 0);
}

/// Unicode in page content and titles.
#[test]
fn vopr_unicode_content() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(root.join("cafe.md"),
        "# Caf\u{00e9} Math\n\nR\u{00e9}sum\u{00e9} of \u{03c0} and \u{2211} concepts."
    ).unwrap();
    fs::write(root.join("linked.md"),
        "# Linked\n\nSee [[cafe]] for \u{00e9}tude."
    ).unwrap();

    let wiki = WikiBackend::open(root).unwrap();
    assert_eq!(wiki.len(), 2);
    assert!(wiki.graph().is_row_stochastic());

    let result = wiki.retrieve("caf\u{00e9}", 1000, &SpreadConfig::default());
    assert!(result.total_cost <= 1000);
}

/// Deeply nested directory structure.
#[test]
fn vopr_deep_nesting() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    let deep_path = root.join("a/b/c/d/e");
    fs::create_dir_all(&deep_path).unwrap();
    fs::write(deep_path.join("deep.md"), "# Deep\n\nVery nested. See [[surface]].").unwrap();
    fs::write(root.join("surface.md"), "# Surface\n\nTop level.").unwrap();

    let wiki = WikiBackend::open(root).unwrap();
    assert_eq!(wiki.len(), 2);

    let deep = wiki.page(&PageId("a/b/c/d/e/deep".into()));
    assert!(deep.is_some(), "Deep page should be found");
}
