//! Quick timing harness — not criterion, just stdout numbers.
//!
//! Run via: cargo run --release --bench bench

use std::time::Instant;

use wiki_backend::WikiBackend;
use spread::{SpreadConfig, SigmoidThreshold, spread as do_spread};

fn main() {
    // Cargo passes "--bench" as argv[1]; take first non-flag arg.
    let root = std::env::args().skip(1)
        .find(|a| !a.starts_with("--"))
        .unwrap_or_else(|| "/home/ops/Project/kahea/cowiki-rs/wiki-corpus/game-theory".into());
    eprintln!("opening {root}");
    let wiki = WikiBackend::open(&root).unwrap();
    let n = wiki.len();
    let g = wiki.graph();
    let (rp, ci, v) = g.adj_transpose_csr();
    let nnz = v.len();
    eprintln!("n={n} nnz={nnz} density={:.4}%", (nnz as f64) / ((n * n) as f64) * 100.0);

    // Warm up.
    let cfg = SpreadConfig::default();
    let thresh = SigmoidThreshold::default();
    let mut init = vec![0.0; n];
    for i in 0..10.min(n) {
        init[i] = 1.0 / 10.0;
    }

    // Time spread alone.
    let mut total = 0.0f64;
    let runs = 50;
    for _ in 0..runs {
        let t = Instant::now();
        let r = do_spread(g, &init, &thresh, &cfg);
        std::hint::black_box(&r);
        total += t.elapsed().as_secs_f64();
    }
    println!("spread only:  avg {:.3} ms  ({} iters avg)",
        total / runs as f64 * 1000.0,
        {
            let r = do_spread(g, &init, &thresh, &cfg);
            r.iterations
        });

    // Time retrieve (full pipeline: ignite + spread + select).
    let mut total = 0.0f64;
    for _ in 0..runs {
        let t = Instant::now();
        let r = wiki.retrieve("prisoner dilemma cooperation", 2000, &cfg);
        std::hint::black_box(&r);
        total += t.elapsed().as_secs_f64();
    }
    println!("full retrieve: avg {:.3} ms", total / runs as f64 * 1000.0);

    // Manual SpMV timing.
    let iters = 30;
    let mut current = vec![1.0 / n as f64; n];
    let t = Instant::now();
    for _ in 0..iters {
        let mut next = vec![0.0f64; n];
        for j in 0..n {
            let mut s: f64 = 0.0;
            for k in rp[j]..rp[j + 1] {
                s += v[k] as f64 * current[ci[k]];
            }
            next[j] = s;
        }
        current = next;
    }
    let e = t.elapsed();
    println!("raw SpMV × {} iters: {:.3} ms total ({:.4} ms/iter, nnz={})",
        iters, e.as_secs_f64() * 1000.0, e.as_secs_f64() * 1000.0 / iters as f64, nnz);
}
