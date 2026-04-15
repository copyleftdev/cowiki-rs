//! Ephemeral simulation engine.
//!
//! Generates a large wiki on-the-fly in a temp directory, runs hundreds
//! of mixed operations, and streams per-operation telemetry as SSE events.

use std::time::Instant;

use serde::Serialize;
use wiki_backend::types::PageId;
use wiki_backend::WikiBackend;
use spread::SpreadConfig;
use temporal_graph::RemConfig;

const WORDS: &[&str] = &[
    "neural", "network", "transformer", "attention", "gradient", "backprop",
    "convolution", "embedding", "tokenizer", "decoder", "encoder", "diffusion",
    "reinforcement", "policy", "reward", "kernel", "activation", "sigmoid",
    "softmax", "dropout", "batch", "epoch", "tensor", "matrix", "vector",
    "sparse", "dense", "latent", "manifold", "topology", "graph", "node",
    "edge", "weight", "bias", "memory", "cache", "retrieval", "spreading",
    "decay", "pruning", "dreaming", "backlink", "category", "wiki", "article",
    "knowledge", "semantic", "cosine", "similarity", "contraction", "convergence",
    "threshold", "sigmoid", "entropy", "divergence", "inference", "posterior",
    "likelihood", "bayesian", "markov", "stochastic", "deterministic", "chaos",
    "equilibrium", "stability", "oscillation", "damping", "resonance", "harmonic",
    "frequency", "amplitude", "phase", "spectrum", "fourier", "wavelet",
    "compression", "entropy", "redundancy", "capacity", "throughput", "latency",
    "bandwidth", "congestion", "routing", "protocol", "handshake", "encryption",
    "authentication", "authorization", "certificate", "signature", "hash", "digest",
    "collision", "preimage", "commitment", "proof", "verification", "witness",
];

const DIRS: &[&str] = &[
    "", "ai/", "math/", "systems/", "security/", "distributed/",
    "cognitive/", "physics/", "biology/", "economics/", "linguistics/",
    "philosophy/", "engineering/", "research/", "notes/", "projects/",
];

const QUERIES: &[&str] = &[
    "neural network convergence",
    "memory consolidation sleep",
    "attack surface vulnerability",
    "distributed consensus fault",
    "graph traversal activation",
    "bayesian inference posterior",
    "encryption authentication hash",
    "transformer attention mechanism",
    "stochastic markov equilibrium",
    "compression entropy bandwidth",
    "cognitive priming association",
    "cache retrieval latency",
    "topology manifold embedding",
    "reinforcement policy reward",
    "wavelet fourier spectrum",
    "threshold sigmoid contraction",
    "protocol handshake routing",
    "pruning decay dreaming",
    "knowledge semantic similarity",
    "oscillation damping stability",
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
    fn usize(&mut self, max: usize) -> usize { (self.next_u64() as usize) % max }
    fn word(&mut self) -> &'static str { WORDS[self.usize(WORDS.len())] }
    fn query(&mut self) -> &'static str { QUERIES[self.usize(QUERIES.len())] }
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum Event {
    #[serde(rename = "seed")]
    Seed {
        page_count: usize,
        edge_count: usize,
        density: f64,
        elapsed_us: u64,
    },
    #[serde(rename = "query")]
    Query {
        query: String,
        results: usize,
        score: f64,
        cost: u64,
        iterations: usize,
        converged: bool,
        elapsed_us: u64,
    },
    #[serde(rename = "maintain")]
    Maintain {
        health: f64,
        pruned: usize,
        dreamed: usize,
        dreamed_edges: Vec<[String; 2]>,
        elapsed_us: u64,
    },
    #[serde(rename = "create")]
    Create {
        id: String,
        title: String,
        tokens: u64,
        links: usize,
        page_count: usize,
        edge_count: usize,
        elapsed_us: u64,
    },
    #[serde(rename = "done")]
    Done {
        total_ops: usize,
        total_us: u64,
        query_count: usize,
        query_avg_us: f64,
        query_p50_us: u64,
        query_p95_us: u64,
        query_p99_us: u64,
        maintain_count: usize,
        create_count: usize,
        final_pages: usize,
        final_edges: usize,
        final_health: f64,
    },
}

/// Run the full simulation. Returns events one at a time via the callback.
/// Inputs are clamped to prevent resource exhaustion.
pub fn run_simulation<F>(n_seed_pages: usize, n_ops: usize, mut emit: F)
where
    F: FnMut(Event),
{
    let n_seed_pages = n_seed_pages.clamp(5, 500);
    let n_ops = n_ops.clamp(10, 5000);

    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path();
    let mut rng = Rng::new(0xCAFE_BEEF);

    // Phase 1: Generate seed wiki.
    let t0 = Instant::now();
    let mut page_ids: Vec<String> = Vec::new();

    for _ in 0..n_seed_pages {
        let dir = DIRS[rng.usize(DIRS.len())];
        let name = format!("{}-{}", rng.word(), rng.word());
        let id = format!("{dir}{name}");
        let title: String = (0..3).map(|_| rng.word()).collect::<Vec<_>>().join(" ");

        let mut content = format!("# {title}\n\n");
        for _ in 0..4 + rng.usize(8) {
            let sentence: String = (0..5 + rng.usize(10))
                .map(|_| rng.word())
                .collect::<Vec<_>>()
                .join(" ");
            content.push_str(&sentence);
            content.push_str(". ");
        }

        let n_links = rng.usize(5);
        for _ in 0..n_links {
            if !page_ids.is_empty() {
                let target = &page_ids[rng.usize(page_ids.len())];
                content.push_str(&format!(" See [[{target}]]."));
            }
        }

        let path = root.join(format!("{id}.md"));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, &content).unwrap();
        page_ids.push(id);
    }

    let mut wiki = WikiBackend::open(root).unwrap();
    let seed_elapsed = t0.elapsed().as_micros() as u64;

    let g = wiki.graph();
    let n = g.len();
    let edge_count = (0..n)
        .flat_map(|i| (0..n).map(move |j| (i, j)))
        .filter(|&(i, j)| g.raw_weight(i, j) > 0.0)
        .count();
    let max_e = if n > 1 { n * (n - 1) } else { 1 };

    emit(Event::Seed {
        page_count: n,
        edge_count,
        density: edge_count as f64 / max_e as f64,
        elapsed_us: seed_elapsed,
    });

    // Phase 2: Mixed operations.
    let spread_cfg = SpreadConfig::default();
    let rem_cfg = RemConfig {
        decay_rate: 0.03,
        prune_threshold: 0.005,
        prune_window: 10,
        d: 0.8,
        activation_threshold: 0.01,
    };

    let mut query_latencies: Vec<u64> = Vec::new();
    let mut maintain_count = 0usize;
    let mut create_count = 0usize;
    let mut total_us = 0u64;
    let mut last_health = 1.0f64;

    for _ in 0..n_ops {
        let op = rng.usize(10);
        match op {
            0..=5 => {
                // Query (60%)
                let q = rng.query().to_string();
                let budget = 500 + rng.usize(3000) as u64;
                let t = Instant::now();
                let result = wiki.retrieve(&q, budget, &spread_cfg);
                let us = t.elapsed().as_micros() as u64;
                query_latencies.push(us);
                total_us += us;

                emit(Event::Query {
                    query: q,
                    results: result.pages.len(),
                    score: result.total_score,
                    cost: result.total_cost,
                    iterations: result.iterations,
                    converged: result.converged,
                    elapsed_us: us,
                });
            }
            6..=7 => {
                // Maintain (20%)
                let t = Instant::now();
                let report = wiki.maintain_with_dream(&rem_cfg);
                let us = t.elapsed().as_micros() as u64;
                maintain_count += 1;
                total_us += us;
                last_health = report.health;

                let dreamed_edges: Vec<[String; 2]> = report.dreamed_edges.iter()
                    .filter_map(|&(src, dst)| {
                        let pages = wiki.all_pages();
                        Some([pages.get(src)?.id.0.clone(), pages.get(dst)?.id.0.clone()])
                    })
                    .collect();

                emit(Event::Maintain {
                    health: report.health,
                    pruned: report.pruned.len(),
                    dreamed: dreamed_edges.len(),
                    dreamed_edges,
                    elapsed_us: us,
                });
            }
            _ => {
                // Create page (20%)
                let dir = DIRS[rng.usize(DIRS.len())];
                let name = format!("{}-{}-{}", rng.word(), rng.word(), create_count);
                let id = format!("{dir}{name}");
                let title: String = (0..3).map(|_| rng.word()).collect::<Vec<_>>().join(" ");
                let mut content = String::new();
                for _ in 0..3 + rng.usize(5) {
                    let s: String = (0..5 + rng.usize(8))
                        .map(|_| rng.word())
                        .collect::<Vec<_>>()
                        .join(" ");
                    content.push_str(&s);
                    content.push_str(". ");
                }
                let n_links = rng.usize(4);
                for _ in 0..n_links {
                    if !page_ids.is_empty() {
                        let target = &page_ids[rng.usize(page_ids.len())];
                        content.push_str(&format!(" See [[{target}]]."));
                    }
                }

                let t = Instant::now();
                wiki.create_page(&PageId(id.clone()), &title, &content).unwrap();
                let us = t.elapsed().as_micros() as u64;
                create_count += 1;
                total_us += us;
                page_ids.push(id.clone());

                let g = wiki.graph();
                let n = g.len();
                let ec = (0..n)
                    .flat_map(|i| (0..n).map(move |j| (i, j)))
                    .filter(|&(i, j)| g.raw_weight(i, j) > 0.0)
                    .count();

                let tokens = content.len() as u64 / 4;
                emit(Event::Create {
                    id,
                    title,
                    tokens,
                    links: n_links,
                    page_count: n,
                    edge_count: ec,
                    elapsed_us: us,
                });
            }
        }
    }

    // Summary.
    query_latencies.sort();
    let qn = query_latencies.len();
    let final_g = wiki.graph();
    let final_n = final_g.len();
    let final_edges = (0..final_n)
        .flat_map(|i| (0..final_n).map(move |j| (i, j)))
        .filter(|&(i, j)| final_g.raw_weight(i, j) > 0.0)
        .count();

    emit(Event::Done {
        total_ops: n_ops,
        total_us,
        query_count: qn,
        query_avg_us: if qn > 0 { query_latencies.iter().sum::<u64>() as f64 / qn as f64 } else { 0.0 },
        query_p50_us: if qn > 0 { query_latencies[qn / 2] } else { 0 },
        query_p95_us: if qn > 0 { query_latencies[qn * 95 / 100] } else { 0 },
        query_p99_us: if qn > 0 { query_latencies[qn * 99 / 100] } else { 0 },
        maintain_count,
        create_count,
        final_pages: final_n,
        final_edges,
        final_health: last_health,
    });
}
