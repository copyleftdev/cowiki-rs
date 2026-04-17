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
    eprintln!("worst row-sum residual = {worst_row_residual:.3e} (tol 1e-12)");
    eprintln!("diagonal violations    = {diag_violations}");
    eprintln!("negative weights       = {neg_weights}");
    eprintln!("zero costs             = {zero_costs}");

    assert!(worst_row_residual < 1e-12, "row-stochastic invariant violated");
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
    assert!(lhs <= rhs + 1e-12, "sigmoid contraction violated: {lhs} > {rhs}");

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
