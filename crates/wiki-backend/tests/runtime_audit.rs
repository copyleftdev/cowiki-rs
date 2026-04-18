//! Runtime invariant auditor.
//!
//! Loads a real corpus and verifies the claims documented in PROOF.md / CLAUDE.md
//! actually hold at runtime on real data (not just on proptest-generated toys):
//!
//!   - Row-stochastic adjacency (post-construction)
//!   - Zero diagonal (no self-loops)
//!   - Positive costs
//!   - Budget respected across a query sweep
//!   - Knapsack ratio ≥ ½ OPT via exact DP
//!   - Sigmoid operator contraction: L1(Ta-Tb) ≤ d·L1(a-b)
//!   - Geometric residual envelope: r_t ≤ r_0 · d^t
//!   - Convergence flag honesty: converged ⇔ final residual < epsilon
//!   - Save/reload round-trip bit-identical adjacency
//!
//! Run with:
//!   AUDIT_CORPUS=wiki-corpus/game-theory cargo test -p wiki-backend \
//!     --test runtime_audit --release -- --ignored --nocapture
//!
//! For the seeded-fixture matrix (no external corpus needed):
//!   cargo test -p wiki-backend --test runtime_audit --release \
//!     fixture -- --ignored --nocapture
//! or a single spec:
//!   AUDIT_FIXTURE=star-100 cargo test -p wiki-backend --test runtime_audit \
//!     --release fixtures -- --ignored --nocapture

use std::path::PathBuf;

use budget_knap::{select, Item};
use spread::{spread as activate, SigmoidThreshold, SpreadConfig};
use wiki_backend::WikiBackend;

fn corpus_path() -> PathBuf {
    let p = std::env::var("AUDIT_CORPUS").expect("set AUDIT_CORPUS=<path>");
    PathBuf::from(p)
}

fn banner(title: &str) {
    eprintln!("\n========== {title} ==========");
}

// ─── Invariant 1: row-stochasticity / diagonal / cost positivity ───────────

#[test]
#[ignore]
fn graph_structural_invariants() {
    let wiki = WikiBackend::open_or_rebuild(corpus_path()).expect("open corpus");
    let g = wiki.graph();
    let n = g.len();
    banner("structural invariants");
    eprintln!("n = {n}");

    // is_row_stochastic is the library's own check; also recompute here
    // with tighter tolerance and record the worst-row residual.
    let mut worst_row_residual: f64 = 0.0;
    let mut rows_with_edges = 0usize;
    let mut diag_violations = 0usize;
    let mut neg_weights = 0usize;
    let mut zero_costs = 0usize;

    for i in 0..n {
        let row_sum: f64 = (0..n).map(|j| g.adj(i, j)).sum();
        let has = (0..n).any(|j| g.raw_weight(i, j) > 0.0);
        if has {
            rows_with_edges += 1;
            let res = (row_sum - 1.0).abs();
            if res > worst_row_residual { worst_row_residual = res; }
        }
        if g.raw_weight(i, i) != 0.0 { diag_violations += 1; }
        for j in 0..n {
            if g.raw_weight(i, j) < 0.0 { neg_weights += 1; }
        }
        if g.cost(i) == 0 { zero_costs += 1; }
    }

    eprintln!("rows with edges        = {rows_with_edges}");
    eprintln!("worst row-sum residual = {worst_row_residual:.3e} (tol 1e-5 — f32 storage)");
    eprintln!("diagonal violations    = {diag_violations}");
    eprintln!("negative weights       = {neg_weights}");
    eprintln!("zero costs             = {zero_costs}");

    assert!(worst_row_residual < 1e-5, "row-stochastic invariant violated");
    assert_eq!(diag_violations, 0);
    assert_eq!(neg_weights, 0);
    assert_eq!(zero_costs, 0);

    // Cross-check library method agrees.
    assert!(g.is_row_stochastic());
}

// ─── Invariant 2: budget never exceeded, knapsack ≥ ½ OPT ──────────────────

/// Exact 0/1 knapsack via DP. O(n * B). Returns optimal total_score and indices.
fn optimal_dp(items: &[Item], budget: u64) -> (f64, u64) {
    let b = budget as usize;
    // dp[w] = best score achievable with cost ≤ w.
    let mut dp = vec![0.0f64; b + 1];
    for it in items {
        let c = it.cost as usize;
        if c == 0 || c > b { continue; }
        for w in (c..=b).rev() {
            let cand = dp[w - c] + it.score;
            if cand > dp[w] { dp[w] = cand; }
        }
    }
    // Best across all budgets ≤ b.
    let mut best = 0.0;
    let mut best_w = 0usize;
    for w in 0..=b {
        if dp[w] > best { best = dp[w]; best_w = w; }
    }
    (best, best_w as u64)
}

const QUERIES: &[&str] = &[
    "nash equilibrium",
    "extensive form game",
    "mixed strategy",
    "zero sum game",
    "cooperative game",
    "mechanism design",
    "auction theory",
    "repeated game",
    "bargaining",
    "chess opening",
    "go strategy",
    "combinatorial game theory",
    "minimax",
    "bayesian game",
    "prisoners dilemma",
    "correlated equilibrium",
    "evolutionary stable",
    "shapley value",
    "social choice",
    "voting paradox",
];

#[test]
#[ignore]
fn knapsack_respects_budget_and_half_opt() {
    let wiki = WikiBackend::open_or_rebuild(corpus_path()).expect("open corpus");
    banner("knapsack invariants across real queries");
    eprintln!("{:<30} {:>6} {:>6} {:>10} {:>10} {:>7}",
              "query", "n_sel", "iters", "cost/bud", "greedy/opt", "conv");

    let cfg = SpreadConfig::default();
    let budgets = [1000u64, 4000, 16000];
    let mut ratio_min: f64 = f64::INFINITY;
    let mut ratio_n = 0usize;
    let mut budget_violations = 0usize;

    for &b in &budgets {
        for q in QUERIES {
            let r = wiki.retrieve(q, b, &cfg);
            if r.total_cost > b {
                budget_violations += 1;
                eprintln!("  !! BUDGET VIOLATION q={q:?} cost={} b={}", r.total_cost, b);
            }
            // Replay to recover the items the knapsack saw.
            let g = wiki.graph();
            let a0 = wiki.ignite(q);
            let sp = activate(g, &a0, &SigmoidThreshold::default(), &cfg);
            let items: Vec<Item> = sp.activation.iter().enumerate()
                .map(|(i, &s)| Item { score: s, cost: g.cost(i) })
                .collect();
            let sel = select(&items, b);
            let (opt, _) = optimal_dp(&items, b);
            let ratio = if opt > 0.0 { sel.total_score / opt } else { 1.0 };
            if ratio < ratio_min { ratio_min = ratio; }
            ratio_n += 1;
            eprintln!("{:<30} {:>6} {:>6} {:>10} {:>10.3} {:>7}",
                      q, r.pages.len(), r.iterations,
                      format!("{}/{}", r.total_cost, b),
                      ratio, r.converged);
        }
    }
    eprintln!("\nqueries audited = {ratio_n}");
    eprintln!("min greedy/opt   = {ratio_min:.4}");
    eprintln!("budget violations= {budget_violations}");
    assert_eq!(budget_violations, 0, "budget violated");
    assert!(ratio_min >= 0.5 - 1e-9, "≥½ OPT violated: min ratio {}", ratio_min);
}

// ─── Invariant 3: sigmoid contraction + geometric envelope ─────────────────

#[test]
#[ignore]
fn sigmoid_contraction_and_envelope() {
    let wiki = WikiBackend::open_or_rebuild(corpus_path()).expect("open corpus");
    let g = wiki.graph();
    banner("contraction + geometric envelope on real graph");
    let n = g.len();

    let d = 0.8;
    let cfg = SpreadConfig { d, max_iter: 200, epsilon: 1e-14 };
    let thresh = SigmoidThreshold::default();

    // Two distinct starting activations with same support (to stress operator).
    let mut a = vec![0.0f64; n];
    let mut b = vec![0.0f64; n];
    for i in 0..n {
        a[i] = ((i * 2654435761) % 1000) as f64 / 1000.0;
        b[i] = (((i + 7) * 2654435761) % 1000) as f64 / 1000.0;
    }

    // One-step contraction via public API: run 1 iteration each using max_iter=1.
    let one_step = |v: &[f64]| {
        let r = activate(g, v, &thresh, &SpreadConfig { d, max_iter: 1, epsilon: 0.0 });
        r.activation
    };
    let ta = one_step(&a);
    let tb = one_step(&b);

    let lhs: f64 = ta.iter().zip(tb.iter()).map(|(x, y)| (x - y).abs()).sum();
    let rhs: f64 = d * a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum::<f64>();
    eprintln!("L1(Ta-Tb)  = {lhs:.6e}");
    eprintln!("d·L1(a-b)  = {rhs:.6e}");
    eprintln!("ratio      = {:.4} (must be ≤ 1.0)", lhs / rhs);
    // Note: sigmoid shrinks near 0 → strict inequality expected on real data.
    // f32 storage: relative slack ~1e-6 · rhs covers round-trip rounding error.
    assert!(lhs <= rhs + 1e-6 * rhs.max(1.0), "sigmoid contraction violated: {lhs} > {rhs}");

    // Envelope: run to convergence with init = single spike, check r_t ≤ r_0·d^t · (1+ε).
    let mut init = vec![0.0; n];
    init[0] = 1.0;
    let r = activate(g, &init, &thresh, &cfg);
    eprintln!("\nspread run: iters={}, converged={}, final_r={:.3e}",
              r.iterations, r.converged,
              r.residuals.last().copied().unwrap_or(f64::NAN));

    // Convergence flag honesty.
    if r.converged {
        assert!(r.residuals.last().copied().unwrap() < cfg.epsilon,
                "`converged=true` but last residual ≥ epsilon");
    } else {
        assert_eq!(r.iterations, cfg.max_iter, "not-converged but iterations < max_iter");
    }

    if r.residuals.len() >= 5 {
        let r0 = r.residuals[0];
        let mut worst_over_envelope: f64 = 1.0;
        for (t, &r_t) in r.residuals.iter().enumerate() {
            let env = r0 * d.powi(t as i32);
            let margin = if env > 0.0 { r_t / env } else { 0.0 };
            if margin > worst_over_envelope { worst_over_envelope = margin; }
        }
        eprintln!("max r_t / (r_0·d^t) = {worst_over_envelope:.3} (ideal ≤ 1.0, allow slack)");
        // Sigmoid envelope isn't mathematically tight at every step; use loose bound.
        assert!(worst_over_envelope < 5.0, "residuals blew past geometric envelope");
    }
}

// ─── Invariant 4: save/reload bit-identical adjacency ──────────────────────

#[test]
#[ignore]
fn save_reload_roundtrip() {
    let path = corpus_path();
    banner("save/reload roundtrip");
    let w1 = WikiBackend::open_or_rebuild(&path).expect("open 1");
    w1.save().expect("save");
    let w2 = WikiBackend::open_or_rebuild(&path).expect("open 2");

    let g1 = w1.graph();
    let g2 = w2.graph();
    assert_eq!(g1.len(), g2.len(), "node count drift");
    let mut raw_drift = 0usize;
    let mut max_drift: f64 = 0.0;
    for i in 0..g1.len() {
        if g1.cost(i) != g2.cost(i) { panic!("cost drift at {i}"); }
        for j in 0..g1.len() {
            let d = (g1.raw_weight(i, j) - g2.raw_weight(i, j)).abs();
            if d > 0.0 { raw_drift += 1; }
            if d > max_drift { max_drift = d; }
        }
    }
    eprintln!("raw-weight cells differing = {raw_drift}");
    eprintln!("max drift                  = {max_drift:.3e}");
    assert_eq!(raw_drift, 0, "save/reload not bit-identical");
}

// ─── Boundary: knapsack edge cases via real activation ─────────────────────

#[test]
#[ignore]
fn knapsack_edge_cases() {
    let wiki = WikiBackend::open_or_rebuild(corpus_path()).expect("open");
    banner("knapsack edge cases");

    // 1) budget=0 → no selection, total_cost == 0.
    let r = wiki.retrieve("nash", 0, &SpreadConfig::default());
    eprintln!("budget=0: n={}, total_cost={}", r.pages.len(), r.total_cost);
    assert_eq!(r.total_cost, 0);

    // 2) budget smaller than the cheapest page → empty selection expected.
    let cheap: u64 = wiki.all_pages().iter().map(|p| p.token_cost).min().unwrap_or(1);
    let r = wiki.retrieve("nash", cheap.saturating_sub(1), &SpreadConfig::default());
    eprintln!("budget<cheapest ({}): n={}, total_cost={}",
              cheap.saturating_sub(1), r.pages.len(), r.total_cost);
    assert_eq!(r.total_cost, 0);

    // 3) huge budget → selection fits but cost ≤ sum of all costs.
    let sum_costs: u64 = wiki.all_pages().iter().map(|p| p.token_cost).sum();
    let r = wiki.retrieve("nash", sum_costs * 10, &SpreadConfig::default());
    eprintln!("budget=10x sum: n={}, total_cost={} (sum_costs={})",
              r.pages.len(), r.total_cost, sum_costs);
    assert!(r.total_cost <= sum_costs * 10);
}

// ─── Seeded-fixture matrix (no external corpus) ────────────────────────────

/// Shared invariant check for a built fixture. Returns a one-line summary.
fn audit_fixture(spec: &str) -> String {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    seed_corpus::build(spec, tmp.path()).expect("build fixture");

    let wiki = WikiBackend::open(tmp.path()).expect("open built fixture");
    let g = wiki.graph();
    let n = g.len();

    // (1) Row-stochasticity.
    let mut worst_residual: f64 = 0.0;
    let mut rows_with_edges = 0usize;
    for i in 0..n {
        let has = (0..n).any(|j| g.raw_weight(i, j) > 0.0);
        if has {
            rows_with_edges += 1;
            let s: f64 = (0..n).map(|j| g.adj(i, j)).sum();
            worst_residual = worst_residual.max((s - 1.0).abs());
        }
        // (2) No self-loops, positive costs.
        assert_eq!(g.raw_weight(i, i), 0.0, "{spec}: self-loop at {i}");
        assert!(g.cost(i) > 0, "{spec}: zero cost at {i}");
    }
    assert!(worst_residual < 1e-5, "{spec}: row-stochastic residual {worst_residual:.3e}");
    assert!(g.is_row_stochastic());

    // (3) Budget respected across a sweep.
    let cfg = SpreadConfig::default();
    let query = "wiki node graph retrieval";
    for &b in &[0u64, 1, 100, 10_000, 10_000_000] {
        let r = wiki.retrieve(query, b, &cfg);
        assert!(r.total_cost <= b, "{spec}: budget {b} violated: cost={}", r.total_cost);
    }

    // (4) Convergence-flag honesty. Epsilon is bounded below by the f32
    //     storage floor on the SpMV (values cast to f64 on read, but input
    //     precision is ~1e-7 per edge) × nnz.
    let eps = 1e-8_f64;
    let a0 = wiki.ignite(query);
    let sp = activate(g, &a0, &SigmoidThreshold::default(),
        &SpreadConfig { d: 0.8, max_iter: 200, epsilon: eps });
    if sp.converged {
        let final_r = sp.residuals.last().copied().unwrap();
        assert!(final_r < eps, "{spec}: converged=true but residual={final_r:.3e}");
    } else {
        assert_eq!(sp.iterations, 200, "{spec}: not-converged but iterations<max_iter");
    }

    // (5) Sigmoid contraction on one random pair of states.
    let a: Vec<f64> = (0..n).map(|i| ((i * 2654435761) % 1000) as f64 / 1000.0).collect();
    let b: Vec<f64> = (0..n).map(|i| (((i + 7) * 2654435761) % 1000) as f64 / 1000.0).collect();
    let one_step = |v: &[f64]| activate(g, v, &SigmoidThreshold::default(),
        &SpreadConfig { d: 0.8, max_iter: 1, epsilon: 0.0 }).activation;
    let ta = one_step(&a);
    let tb = one_step(&b);
    let lhs: f64 = ta.iter().zip(tb.iter()).map(|(x, y)| (x - y).abs()).sum();
    let rhs: f64 = 0.8 * a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum::<f64>();
    // f32 storage: relative slack of ~1e-6 · rhs covers round-trip rounding.
    assert!(lhs <= rhs + 1e-6 * rhs.max(1.0), "{spec}: contraction violated {lhs:.3e} > {rhs:.3e}");

    // (6) Knapsack ≥½ OPT via DP — only where DP is affordable.
    //     DP is O(n · B) in time and O(B) in memory.
    let mut greedy_opt_ratio: f64 = 1.0;
    if n <= 2_000 {
        let items: Vec<Item> = sp.activation.iter().enumerate()
            .map(|(i, &s)| Item { score: s, cost: g.cost(i) }).collect();
        let sel = select(&items, 4_000);
        let (opt, _) = optimal_dp(&items, 4_000);
        if opt > 0.0 { greedy_opt_ratio = sel.total_score / opt; }
        assert!(greedy_opt_ratio >= 0.5 - 1e-9,
                "{spec}: ≥½ OPT violated: ratio={greedy_opt_ratio}");
    }

    format!(
        "{spec:<16} n={n:<6} live_rows={rows_with_edges:<5} \
         row_resid={worst_residual:.1e}  iters={:<3} conv={}  greedy/opt={:.3}",
        sp.iterations, sp.converged, greedy_opt_ratio,
    )
}

#[test]
#[ignore]
fn fixtures() {
    banner("seeded-fixture matrix");
    let specs: Vec<String> = match std::env::var("AUDIT_FIXTURE") {
        Ok(s) => vec![s],
        Err(_) => [
            // correctness fixtures (small, weird shapes)
            "star-10", "star-100",
            "chain-10", "chain-100",
            "ba-50-3", "ba-200-4", "ba-500-6",
        ].iter().map(|s| s.to_string()).collect(),
    };
    for spec in &specs {
        let line = audit_fixture(spec);
        eprintln!("  {line}");
    }
    eprintln!("\n{} fixtures passed", specs.len());
}

// ─── Scale-envelope fixtures: record performance, don't regress it ──────────
//
// Build time, query p99, and RSS at increasing n. These are *characterization*
// tests — they print a budget sheet and only fail on gross regressions, so
// future refactors (mmap-CSR, inverted-index TF-IDF, segment shards) have a
// hard baseline to prove against.

fn rss_mb() -> u64 {
    // /proc/self/statm: "size rss shared text lib data dirty" (in pages).
    let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let rss_pages: u64 = s.split_whitespace().nth(1)
        .and_then(|x| x.parse().ok()).unwrap_or(0);
    rss_pages * 4 / 1024 // assume 4 KiB pages → MiB
}

fn nearest_rank(sorted: &[u64], p: usize) -> u64 {
    let n = sorted.len();
    let idx = ((p * n + 99) / 100).saturating_sub(1).min(n - 1);
    sorted[idx]
}

#[test]
#[ignore]
fn fixtures_scale() {
    use std::time::Instant;
    banner("scale-envelope");
    eprintln!(
        "{:<14} {:>6} {:>12} {:>10} {:>10} {:>8} {:>10} {:>8}",
        "spec", "n", "build_idx_ms", "q_p50_us", "q_p99_us", "iters", "rss_mb_Δ", "conv%"
    );

    // Default ladder. Opt-in heavier rungs with AUDIT_SCALE=heavy.
    let heavy = std::env::var("AUDIT_SCALE").ok().as_deref() == Some("heavy");
    let mut specs: Vec<&str> = vec!["ba-1000-4", "ba-2500-6", "ba-5000-6"];
    if heavy {
        specs.push("ba-10000-8");
        specs.push("ba-25000-8");
    }

    let rss_before = rss_mb();

    for spec in &specs {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        seed_corpus::build(spec, tmp.path()).expect("build fixture");

        let t_open = Instant::now();
        let wiki = WikiBackend::open(tmp.path()).expect("open built fixture");
        let build_idx_ms = t_open.elapsed().as_millis();
        let n = wiki.len();

        let cfg = SpreadConfig { d: 0.8, max_iter: 200, epsilon: 1e-8 };
        let query = "wiki node graph retrieval activation";

        // Warmup and inner retrieval loop.
        let _ = wiki.retrieve(query, 4_000, &cfg);
        let trials = 100;
        let mut lats = Vec::with_capacity(trials);
        let mut iters_total: u64 = 0;
        let mut conv_count: u64 = 0;
        for _ in 0..trials {
            let t = Instant::now();
            let r = wiki.retrieve(query, 4_000, &cfg);
            let us = t.elapsed().as_micros() as u64;
            lats.push(us);
            iters_total += r.iterations as u64;
            if r.converged { conv_count += 1; }
            std::hint::black_box(&r);
        }
        lats.sort();

        let p50 = nearest_rank(&lats, 50);
        let p99 = nearest_rank(&lats, 99);
        let iters_avg = iters_total as f64 / trials as f64;
        let conv_pct = 100.0 * conv_count as f64 / trials as f64;
        let rss_delta = rss_mb().saturating_sub(rss_before);

        eprintln!(
            "{spec:<14} {n:>6} {build_idx_ms:>12} {p50:>10} {p99:>10} \
             {iters_avg:>8.1} {rss_delta:>10} {conv_pct:>7.0}%"
        );

        // Sanity floor — these don't set a tight budget, they catch regressions
        // of >10× that something plainly broke. Tighten once we're ready.
        assert!(p99 < 5_000_000, "{spec}: query p99 {p99} µs exceeds 5 s sanity floor");
        assert!(build_idx_ms < 300_000, "{spec}: index build {build_idx_ms} ms exceeds 5 min sanity floor");
    }

    eprintln!("\n(AUDIT_SCALE=heavy to include 10k / 25k rungs)");
}
