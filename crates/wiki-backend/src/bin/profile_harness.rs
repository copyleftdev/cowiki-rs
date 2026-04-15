//! Profiling harness: exercises the full wiki-backend pipeline in a tight loop.
//!
//! Usage:
//!   cargo build --release -p wiki-backend --bin profile_harness
//!   perf record ./target/release/profile_harness
//!   perf report
//!
//! Or with flamegraph:
//!   cargo flamegraph --bin profile_harness -p wiki-backend

use std::fs;
use std::time::Instant;

use wiki_backend::types::PageId;
use wiki_backend::WikiBackend;
use spread::SpreadConfig;
use temporal_graph::RemConfig;

const WORDS: &[&str] = &[
    "neural", "network", "transformer", "attention", "gradient",
    "backprop", "convolution", "embedding", "tokenizer", "decoder",
    "encoder", "diffusion", "reinforcement", "policy", "reward",
    "kernel", "activation", "sigmoid", "softmax", "dropout",
    "batch", "epoch", "tensor", "matrix", "vector",
    "sparse", "dense", "latent", "manifold", "topology",
    "graph", "node", "edge", "weight", "bias",
    "memory", "cache", "retrieval", "spreading", "decay",
    "pruning", "dreaming", "backlink", "category", "wiki",
    "article", "knowledge", "semantic", "cosine", "similarity",
];

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
    fn word(&mut self) -> &'static str {
        WORDS[self.next_usize(WORDS.len())]
    }
}

fn main() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let mut rng = Rng::new(0xBEEF_CAFE);

    eprintln!("=== Phase 1: Build a 200-page wiki ===");
    let t0 = Instant::now();

    // Create pages with interconnected backlinks.
    let mut page_ids: Vec<String> = Vec::new();
    for i in 0..200 {
        let dirs = ["", "ai/", "math/", "systems/", "notes/", "research/"];
        let dir = dirs[rng.next_usize(dirs.len())];
        let name = format!("{}-{}", rng.word(), rng.word());
        let id = format!("{dir}{name}");
        let title: String = (0..3).map(|_| rng.word()).collect::<Vec<_>>().join(" ");

        let mut content = format!("# {title}\n\n");
        for _ in 0..5 + rng.next_usize(10) {
            let sentence: String = (0..6 + rng.next_usize(8))
                .map(|_| rng.word())
                .collect::<Vec<_>>()
                .join(" ");
            content.push_str(&sentence);
            content.push_str(". ");
        }

        // Add backlinks to existing pages.
        let n_links = rng.next_usize(4);
        for _ in 0..n_links {
            if !page_ids.is_empty() {
                let target = &page_ids[rng.next_usize(page_ids.len())];
                content.push_str(&format!("\n\nSee [[{target}]] for related work."));
            }
        }

        let path = root.join(format!("{id}.md"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, &content).unwrap();
        page_ids.push(id);
    }
    eprintln!("  Created 200 pages in {:?}", t0.elapsed());

    // ─── Phase 2: Open and index ─────────────────────────────────────────
    eprintln!("\n=== Phase 2: Open and index ===");
    let t1 = Instant::now();
    let mut wiki = WikiBackend::open(root).unwrap();
    eprintln!("  Opened and indexed {} pages in {:?}", wiki.len(), t1.elapsed());

    // ─── Phase 3: Retrieval benchmark ────────────────────────────────────
    eprintln!("\n=== Phase 3: 1000 queries ===");
    let spread_cfg = SpreadConfig { d: 0.8, max_iter: 100, epsilon: 1e-8 };
    let t2 = Instant::now();

    for _ in 0..1000 {
        let query: String = (0..2 + rng.next_usize(3))
            .map(|_| rng.word())
            .collect::<Vec<_>>()
            .join(" ");
        let budget = 200 + rng.next_usize(1000) as u64;
        let result = wiki.retrieve(&query, budget, &spread_cfg);
        // Prevent optimizer from eliding the work.
        std::hint::black_box(&result);
    }
    let query_elapsed = t2.elapsed();
    eprintln!("  1000 queries in {:?} ({:.1} us/query)",
        query_elapsed,
        query_elapsed.as_micros() as f64 / 1000.0);

    // ─── Phase 4: REM maintenance ────────────────────────────────────────
    eprintln!("\n=== Phase 4: 100 REM cycles ===");
    let rem_cfg = RemConfig {
        decay_rate: 0.03,
        prune_threshold: 0.001,
        prune_window: 10,
        d: 0.8,
        activation_threshold: 0.01,
    };
    let t3 = Instant::now();

    for _ in 0..100 {
        let report = wiki.maintain(&rem_cfg);
        std::hint::black_box(&report);
    }
    let rem_elapsed = t3.elapsed();
    eprintln!("  100 REM cycles in {:?} ({:.1} ms/cycle)",
        rem_elapsed,
        rem_elapsed.as_millis() as f64 / 100.0);

    // ─── Phase 5: REM with dream ─────────────────────────────────────────
    eprintln!("\n=== Phase 5: 20 REM+dream cycles ===");
    let t4 = Instant::now();

    for _ in 0..20 {
        let report = wiki.maintain_with_dream(&rem_cfg);
        std::hint::black_box(&report);
    }
    let dream_elapsed = t4.elapsed();
    eprintln!("  20 REM+dream cycles in {:?} ({:.1} ms/cycle)",
        dream_elapsed,
        dream_elapsed.as_millis() as f64 / 20.0);

    // ─── Phase 6: Save/reload ────────────────────────────────────────────
    eprintln!("\n=== Phase 6: 50 save/reload cycles ===");
    let t5 = Instant::now();

    for _ in 0..50 {
        wiki.save().unwrap();
        wiki = WikiBackend::open_or_rebuild(root).unwrap();
    }
    let persist_elapsed = t5.elapsed();
    eprintln!("  50 save/reload cycles in {:?} ({:.1} ms/cycle)",
        persist_elapsed,
        persist_elapsed.as_millis() as f64 / 50.0);

    // ─── Phase 7: Page creation churn ────────────────────────────────────
    eprintln!("\n=== Phase 7: Create 50 more pages (with rebuild) ===");
    let t6 = Instant::now();

    for i in 0..50 {
        let id = format!("churn/page-{i}");
        let content = format!("Content for churn page {i}. See [[{}]].",
            page_ids[rng.next_usize(page_ids.len())]);
        wiki.create_page(&PageId(id), &format!("Churn {i}"), &content).unwrap();
    }
    let churn_elapsed = t6.elapsed();
    eprintln!("  50 page creates (with rebuild) in {:?} ({:.1} ms/create)",
        churn_elapsed,
        churn_elapsed.as_millis() as f64 / 50.0);

    // ─── Summary ─────────────────────────────────────────────────────────
    eprintln!("\n=== Summary ===");
    eprintln!("  Wiki size:       {} pages", wiki.len());
    eprintln!("  Query:           {:.1} us/query", query_elapsed.as_micros() as f64 / 1000.0);
    eprintln!("  REM:             {:.1} ms/cycle", rem_elapsed.as_millis() as f64 / 100.0);
    eprintln!("  REM+dream:       {:.1} ms/cycle", dream_elapsed.as_millis() as f64 / 20.0);
    eprintln!("  Save/reload:     {:.1} ms/cycle", persist_elapsed.as_millis() as f64 / 50.0);
    eprintln!("  Create+rebuild:  {:.1} ms/create", churn_elapsed.as_millis() as f64 / 50.0);
}
