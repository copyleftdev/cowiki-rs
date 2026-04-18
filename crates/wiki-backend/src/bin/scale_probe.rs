//! Subprocess-isolated scale probe.
//!
//! Runs ONE rung of the scale-envelope test in a fresh process and prints
//! a single JSON line to stdout. The parent test harness invokes this
//! binary once per rung, so RSS measurements are per-rung rather than
//! cumulative (glibc doesn't release freed pages back to the OS, so
//! running multiple rungs in one process inflates the later rungs'
//! reported memory).
//!
//! Usage:
//!   scale_probe <spec>
//!
//! `spec` is anything `seed_corpus::build` accepts: `star-100`, `ba-1000-4`,
//! `clique-200`, etc.
//!
//! Output JSON schema:
//!   {"spec": "ba-1000-4", "n": 1000, "build_idx_ms": 77,
//!    "q_p50_us": 840, "q_p99_us": 950,
//!    "rebuild_ms": 1, "update_ms": 0,
//!    "save_ms": 50, "load_ms": 20,
//!    "iters_avg": 93.0, "converged_pct": 100.0, "rss_mb": 45}

use std::time::Instant;

use wiki_backend::WikiBackend;
use spread::SpreadConfig;

fn rss_mb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let pages: u64 = s.split_whitespace().nth(1)
        .and_then(|x| x.parse().ok()).unwrap_or(0);
    pages * 4 / 1024
}

fn nearest_rank(sorted: &[u64], p: usize) -> u64 {
    let n = sorted.len();
    let idx = ((p * n + 99) / 100).saturating_sub(1).min(n - 1);
    sorted[idx]
}

fn main() {
    let spec = std::env::args().nth(1).expect("usage: scale_probe <spec>");

    let tmp = tempfile::TempDir::new().expect("tempdir");
    seed_corpus::build(&spec, tmp.path()).expect("build fixture");

    // Build index (cold start from markdown).
    let t_open = Instant::now();
    let mut wiki = WikiBackend::open(tmp.path()).expect("open");
    let build_idx_ms = t_open.elapsed().as_millis();
    let n = wiki.len();

    // Query latency distribution.
    let cfg = SpreadConfig { d: 0.8, max_iter: 200, epsilon: 1e-8 };
    let query = "wiki node graph retrieval activation";
    let _ = wiki.retrieve(query, 4_000, &cfg); // warmup
    let trials = 100usize;
    let mut lats = Vec::with_capacity(trials);
    let mut iters_total: u64 = 0;
    let mut conv_count: u64 = 0;
    for _ in 0..trials {
        let t = Instant::now();
        let r = wiki.retrieve(query, 4_000, &cfg);
        lats.push(t.elapsed().as_micros() as u64);
        iters_total += r.iterations as u64;
        if r.converged { conv_count += 1; }
        std::hint::black_box(&r);
    }
    lats.sort();
    let p50 = nearest_rank(&lats, 50);
    let p99 = nearest_rank(&lats, 99);

    // Write path: single create.
    let page_id = wiki_backend::types::PageId(format!("audit-probe-{spec}"));
    let t = Instant::now();
    wiki.create_page(&page_id, "probe", "Just a probe.").expect("create");
    let rebuild_ms = t.elapsed().as_millis();

    // Update path: edit page 0.
    let upd_id = wiki.all_pages()[0].id.clone();
    let new_content = format!("# probe update\n\nRefreshed body.\n");
    let t = Instant::now();
    wiki.update_page(&upd_id, &new_content).expect("update");
    let update_ms = t.elapsed().as_millis();

    // Persistence round-trip.
    let t = Instant::now();
    wiki.save().expect("save");
    let save_ms = t.elapsed().as_millis();
    drop(wiki);

    let t = Instant::now();
    let reloaded = WikiBackend::open_or_rebuild(tmp.path()).expect("reload");
    let load_ms = t.elapsed().as_millis();
    std::hint::black_box(&reloaded);

    let iters_avg = iters_total as f64 / trials as f64;
    let conv_pct = 100.0 * conv_count as f64 / trials as f64;
    let rss = rss_mb();

    println!(
        "{{\"spec\":\"{spec}\",\"n\":{n},\"build_idx_ms\":{build_idx_ms},\
         \"q_p50_us\":{p50},\"q_p99_us\":{p99},\
         \"rebuild_ms\":{rebuild_ms},\"update_ms\":{update_ms},\
         \"save_ms\":{save_ms},\"load_ms\":{load_ms},\
         \"iters_avg\":{iters_avg:.1},\"converged_pct\":{conv_pct:.1},\
         \"rss_mb\":{rss}}}"
    );
}
