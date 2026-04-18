//! # seed-corpus
//!
//! Deterministic adversarial-corpus builders.
//!
//! Each builder writes a directory of `.md` files that `WikiBackend::open`
//! can ingest directly. Shapes are chosen to isolate one pathological
//! property of the retrieval pipeline so the runtime-audit harness can
//! pin down where an invariant bends under pressure.
//!
//! Seeds are fixed; rebuilds are bit-identical. Output is on-disk so the
//! full `scan → parse → tfidf → graph → persist` path is exercised — the
//! shortest route to "end-to-end under a pathological shape."
//!
//! ## Shapes currently modelled
//!
//! - `star(n)` — one hub linking to (n−1) leaves. Stresses activation
//!   concentration and the out-degree=0 / out-degree=(n−1) extremes.
//! - `chain(n)` — 1 → 2 → … → n. Stresses depth-bounded convergence;
//!   `max_iter` has to cover diameter.
//! - `ba(n, m)` — Barabási-Albert scale-free. Baseline for real-wiki
//!   shape: hubs + long tail, preferential attachment.
//!
//! ## Dispatcher
//!
//! [`build`] parses a spec string like `"star-100"`, `"chain-500"`, or
//! `"ba-1000-4"` and writes the fixture. Useful for parameterising tests
//! and benchmarks via one environment variable.

use std::fs;
use std::io;
use std::path::Path;

/// Deterministic RNG. Same xorshift as `profile_harness` — no external
/// dependency so this crate stays dependency-free.
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
        (self.next_u64() as usize) % max.max(1)
    }
}

/// Parameters shared across all builders.
#[derive(Clone, Copy, Debug)]
pub struct SeedConfig {
    pub seed: u64,
    /// Filler sentences per page (makes TF-IDF non-degenerate).
    pub sentences_per_page: usize,
}

impl Default for SeedConfig {
    fn default() -> Self {
        Self { seed: 0xC0_FFEE_u64, sentences_per_page: 4 }
    }
}

/// Pool of common tokens so TF-IDF has real vocabulary to index.
const COMMON_WORDS: &[&str] = &[
    "wiki", "node", "edge", "graph", "retrieval", "activation", "spreading",
    "threshold", "budget", "token", "score", "memory", "attention", "decay",
    "dream", "cycle", "health", "temporal", "invariant", "contraction",
    "convergence", "fixed", "point", "sigmoid", "density", "sparse", "dense",
    "cache", "pipeline", "backlink", "category", "chunk", "quality", "rank",
];

fn filler(rng: &mut Rng, sentences: usize) -> String {
    let mut out = String::new();
    for _ in 0..sentences {
        let words = 5 + rng.next_usize(8);
        for k in 0..words {
            if k > 0 { out.push(' '); }
            out.push_str(COMMON_WORDS[rng.next_usize(COMMON_WORDS.len())]);
        }
        out.push_str(". ");
    }
    out.push('\n');
    out
}

fn page_id(i: usize, n: usize) -> String {
    // zero-pad to the width of n so lexicographic order == numeric order.
    let width = format!("{}", n.saturating_sub(1)).len().max(1);
    format!("page-{:0width$}", i, width = width)
}

fn write_page(
    root: &Path,
    rng: &mut Rng,
    i: usize,
    n: usize,
    title: &str,
    links: &[usize],
    cfg: &SeedConfig,
) -> io::Result<()> {
    let id = page_id(i, n);
    let mut content = format!("# {title}\n\n");
    // Sprinkle a unique topic token per page so ignite has signal.
    content.push_str(&format!("topic-{i:05} {}\n\n", filler(rng, cfg.sentences_per_page)));
    for &tgt in links {
        content.push_str(&format!("See [[{}]].\n", page_id(tgt, n)));
    }
    fs::write(root.join(format!("{id}.md")), content)
}

/// Hub-and-spoke: page-0 is a hub linking to the other (n−1) leaves.
///
/// Leaves have no outgoing links (out-degree 0 → row-zero in adjacency).
/// This stresses the row-stochastic fallback (rows with zero out-degree
/// must stay all-zero) and spreading's convergence when activation
/// collects at the hub.
pub fn write_star(root: &Path, n: usize, cfg: &SeedConfig) -> io::Result<()> {
    assert!(n >= 2, "star needs at least 2 nodes");
    fs::create_dir_all(root)?;
    let mut rng = Rng::new(cfg.seed);

    // Hub links to every leaf.
    let leaves: Vec<usize> = (1..n).collect();
    write_page(root, &mut rng, 0, n, "Hub", &leaves, cfg)?;

    for i in 1..n {
        write_page(root, &mut rng, i, n, &format!("Leaf {i}"), &[], cfg)?;
    }
    Ok(())
}

/// Linear chain: i → i+1 for i in 0..n−1. Diameter = n−1.
///
/// Tests that spreading converges within `max_iter` when signal has to
/// travel the full depth. At n > max_iter, activation can't reach the
/// tail — the `converged=true` claim has to be honest about that.
pub fn write_chain(root: &Path, n: usize, cfg: &SeedConfig) -> io::Result<()> {
    assert!(n >= 2, "chain needs at least 2 nodes");
    fs::create_dir_all(root)?;
    let mut rng = Rng::new(cfg.seed);

    for i in 0..n {
        let links: Vec<usize> = if i + 1 < n { vec![i + 1] } else { vec![] };
        write_page(root, &mut rng, i, n, &format!("Step {i}"), &links, cfg)?;
    }
    Ok(())
}

/// Barabási-Albert scale-free graph: start from a small clique of `m+1`
/// seed nodes (fully connected), then each newly-added node attaches `m`
/// edges to existing nodes with probability proportional to their
/// current degree. Produces a power-law degree distribution — realistic
/// wiki shape, with natural hubs.
pub fn write_ba(root: &Path, n: usize, m: usize, cfg: &SeedConfig) -> io::Result<()> {
    assert!(n > m, "need n > m");
    assert!(m >= 1, "m must be ≥ 1");
    fs::create_dir_all(root)?;
    let mut rng = Rng::new(cfg.seed);

    // Preferential-attachment "bag": each existing node appears once per
    // edge it has. Drawing uniformly from the bag == drawing proportional
    // to degree.
    let mut bag: Vec<usize> = Vec::with_capacity(2 * n * m);
    let mut links: Vec<Vec<usize>> = vec![Vec::new(); n];

    // Seed: clique on nodes 0..=m.
    for i in 0..=m {
        for j in 0..=m {
            if i != j {
                links[i].push(j);
                bag.push(i);
            }
        }
    }

    for v in (m + 1)..n {
        // Choose m distinct targets from the bag.
        let mut picked: Vec<usize> = Vec::with_capacity(m);
        let mut attempts = 0;
        while picked.len() < m && attempts < 10 * m {
            attempts += 1;
            let idx = rng.next_usize(bag.len());
            let t = bag[idx];
            if t != v && !picked.contains(&t) {
                picked.push(t);
            }
        }
        for &t in &picked {
            links[v].push(t);
            bag.push(v);
            bag.push(t);
        }
    }

    for i in 0..n {
        write_page(root, &mut rng, i, n, &format!("Node {i}"), &links[i], cfg)?;
    }
    Ok(())
}

/// Parse a spec string and build the corresponding fixture.
///
/// Specs:
/// - `star-N`
/// - `chain-N`
/// - `ba-N-M`
pub fn build(spec: &str, root: &Path) -> io::Result<()> {
    let cfg = SeedConfig::default();
    let parts: Vec<&str> = spec.split('-').collect();
    match parts.as_slice() {
        ["star", n] => {
            let n: usize = parse_usize(n)?;
            write_star(root, n, &cfg)
        }
        ["chain", n] => {
            let n: usize = parse_usize(n)?;
            write_chain(root, n, &cfg)
        }
        ["ba", n, m] => {
            let n: usize = parse_usize(n)?;
            let m: usize = parse_usize(m)?;
            write_ba(root, n, m, &cfg)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown fixture spec: {spec} (expected star-N | chain-N | ba-N-M)"),
        )),
    }
}

fn parse_usize(s: &str) -> io::Result<usize> {
    s.parse::<usize>().map_err(|_| io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("not a positive integer: {s}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn count_md(root: &Path) -> usize {
        fs::read_dir(root).unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
            .count()
    }

    #[test]
    fn star_has_hub_and_leaves() {
        let tmp = tempfile::tempdir().unwrap();
        write_star(tmp.path(), 10, &SeedConfig::default()).unwrap();
        assert_eq!(count_md(tmp.path()), 10);
        let hub = fs::read_to_string(tmp.path().join("page-0.md")).unwrap();
        // Hub should mention each leaf.
        for i in 1..10 {
            assert!(hub.contains(&format!("[[page-{i}]]")), "hub missing leaf {i}");
        }
        let leaf = fs::read_to_string(tmp.path().join("page-5.md")).unwrap();
        assert!(!leaf.contains("[["), "leaf should have no outgoing links");
    }

    #[test]
    fn chain_links_forward() {
        let tmp = tempfile::tempdir().unwrap();
        write_chain(tmp.path(), 5, &SeedConfig::default()).unwrap();
        let p2 = fs::read_to_string(tmp.path().join("page-2.md")).unwrap();
        assert!(p2.contains("[[page-3]]"));
        let last = fs::read_to_string(tmp.path().join("page-4.md")).unwrap();
        assert!(!last.contains("[["), "chain tail has no outgoing link");
    }

    #[test]
    fn ba_is_connected() {
        let tmp = tempfile::tempdir().unwrap();
        write_ba(tmp.path(), 20, 3, &SeedConfig::default()).unwrap();
        assert_eq!(count_md(tmp.path()), 20);
        // Every non-seed node should have at least one outgoing link.
        for i in 4..20 {
            let p = fs::read_to_string(tmp.path().join(format!("page-{i:02}.md"))).unwrap();
            assert!(p.contains("[["), "BA node {i} has no outbound link");
        }
    }

    #[test]
    fn build_dispatch() {
        let tmp = tempfile::tempdir().unwrap();
        build("star-8", tmp.path()).unwrap();
        assert_eq!(count_md(tmp.path()), 8);
    }

    #[test]
    fn deterministic() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_ba(a.path(), 15, 2, &SeedConfig::default()).unwrap();
        write_ba(b.path(), 15, 2, &SeedConfig::default()).unwrap();
        for i in 0..15 {
            let fa = fs::read_to_string(a.path().join(format!("page-{i:02}.md"))).unwrap();
            let fb = fs::read_to_string(b.path().join(format!("page-{i:02}.md"))).unwrap();
            assert_eq!(fa, fb, "non-deterministic at page {i}");
        }
    }
}
