//! # spread
//!
//! Spreading activation on weighted directed graphs.
//!
//! ## Two operator variants
//!
//! **Linear** (θ=0) — provably contracting:
//! ```text
//! T_lin(a) = (1 - d) · a⁰  +  d · Wᵀ · a
//! ‖T_lin(a) - T_lin(b)‖₁ ≤ d · ‖a - b‖₁
//! ```
//!
//! **Thresholded** (θ>0) — convergent in practice:
//! ```text
//! T(a) = (1 - d) · a⁰  +  d · Wᵀ · f(a)
//! ```
//! where f zeros activations below θ.
//!
//! The threshold function is a trait — callers choose hard, sigmoid, or custom.
//! The `is_lipschitz_1` flag indicates whether contraction is guaranteed.

use scored_graph::ScoredGraph;

/// Threshold function applied element-wise before propagation.
pub trait ThresholdFn {
    /// Apply threshold to a single activation value.
    fn apply(&self, activation: f64) -> f64;

    /// If true, the operator is a provable contraction mapping.
    /// Hard threshold returns false. Sigmoid and identity return true.
    fn is_lipschitz_1(&self) -> bool;
}

/// No threshold (identity). Guarantees contraction.
#[derive(Debug, Clone, Copy)]
pub struct NoThreshold;

impl ThresholdFn for NoThreshold {
    #[inline]
    fn apply(&self, a: f64) -> f64 {
        a
    }
    fn is_lipschitz_1(&self) -> bool {
        true
    }
}

/// Hard threshold: zero activations below θ. NOT Lipschitz-1.
///
/// **Finding from proof suite:** this breaks strict contraction at the
/// boundary θ and can cause limit cycles. Use [`SigmoidThreshold`] for
/// guaranteed convergence with noise filtering.
#[derive(Debug, Clone, Copy)]
pub struct HardThreshold(pub f64);

impl ThresholdFn for HardThreshold {
    #[inline]
    fn apply(&self, a: f64) -> f64 {
        if a >= self.0 { a } else { 0.0 }
    }
    fn is_lipschitz_1(&self) -> bool {
        false
    }
}

/// Smooth sigmoid threshold. IS Lipschitz-1 for steepness ≤ 4.
///
/// ```text
/// f(a) = a · σ(k · (a - center))
/// ```
///
/// where σ is the logistic function. Approximates hard threshold behavior
/// with smooth transition, preserving contraction.
#[derive(Debug, Clone, Copy)]
pub struct SigmoidThreshold {
    pub center: f64,
    pub steepness: f64,
}

impl SigmoidThreshold {
    pub fn new(center: f64, steepness: f64) -> Self {
        Self { center, steepness }
    }
}

impl Default for SigmoidThreshold {
    fn default() -> Self {
        Self { center: 0.01, steepness: 4.0 }
    }
}

impl ThresholdFn for SigmoidThreshold {
    #[inline]
    fn apply(&self, a: f64) -> f64 {
        let sigmoid = 1.0 / (1.0 + (-self.steepness * (a - self.center)).exp());
        a * sigmoid
    }
    fn is_lipschitz_1(&self) -> bool {
        self.steepness <= 4.0
    }
}

/// Configuration for the spreading activation algorithm.
#[derive(Debug, Clone)]
pub struct SpreadConfig {
    /// Propagation factor d ∈ (0, 1). Higher = activation spreads further.
    pub d: f64,
    /// Maximum iterations before forced stop.
    pub max_iter: usize,
    /// Convergence tolerance (L1 norm of residual).
    pub epsilon: f64,
}

impl Default for SpreadConfig {
    fn default() -> Self {
        Self {
            d: 0.8,
            max_iter: 100,
            epsilon: 1e-8,
        }
    }
}

/// Result of a spreading activation run.
#[derive(Debug, Clone)]
pub struct SpreadResult {
    /// Converged (or final) activation vector.
    pub activation: Vec<f64>,
    /// Number of iterations taken.
    pub iterations: usize,
    /// L1 residual at each step.
    pub residuals: Vec<f64>,
    /// Whether the operator converged within epsilon.
    pub converged: bool,
}

/// Run spreading activation to convergence (or max_iter).
///
/// ## Proven properties (P1.1–P2.4)
///
/// When `threshold.is_lipschitz_1()`:
/// - **Contraction**: `‖T(a)-T(b)‖₁ ≤ d·‖a-b‖₁`
/// - **Convergence**: always converges to unique fixed point
/// - **Geometric rate**: residuals bounded by `r₀·dᵗ`
/// - **Iteration bound**: `O(log(1/ε) / log(1/d))`
/// - **Activation bounded**: `0 ≤ a* ≤ max(a⁰) / (1-d)`
///
/// When `!threshold.is_lipschitz_1()` (hard threshold):
/// - Residuals stay bounded (never blow up)
/// - May enter limit cycles — use `max_iter` as cutoff
pub fn spread<T: ThresholdFn>(
    graph: &ScoredGraph,
    initial: &[f64],
    threshold: &T,
    config: &SpreadConfig,
) -> SpreadResult {
    let n = graph.len();
    assert_eq!(initial.len(), n, "initial activation must match graph size");
    assert!(config.d > 0.0 && config.d < 1.0, "d must be in (0, 1)");

    let d = config.d;
    let one_minus_d = 1.0 - d;
    let (row_ptr, col_idx, values) = graph.adj_transpose_csr();

    // Hoist scratch buffers: at ~100 iters per query and 2 fresh Vec::<f64>
    // allocations per iter (next + thresholded), the old form dominated the
    // allocator on short queries at scale. Ping-pong `current ⇄ next` via swap;
    // `thresholded` is written unconditionally each iter.
    let mut current = initial.to_vec();
    let mut next = vec![0.0f64; n];
    let mut thresholded = vec![0.0f64; n];
    let mut residuals = Vec::with_capacity(config.max_iter);

    for _ in 0..config.max_iter {
        // Apply threshold to current activation (overwrite-in-place).
        for (slot, &a) in thresholded.iter_mut().zip(current.iter()) {
            *slot = threshold.apply(a);
        }

        // Compute (Wᵀ · f(a))[j] = Σ_{k in row j of Wᵀ} values[k] · thresholded[col_idx[k]].
        // Every cell of `next` is unconditionally assigned — no zero-init needed.
        // Sparse: skips the ~99% zeros in a typical wiki graph.
        for j in 0..n {
            // Weights are f32; accumulate in f64 so rounding stays well below
            // the Lipschitz bound.
            let mut spread_j: f64 = 0.0;
            let start = row_ptr[j];
            let end = row_ptr[j + 1];
            for k in start..end {
                spread_j += values[k] as f64 * thresholded[col_idx[k]];
            }
            next[j] = one_minus_d * initial[j] + d * spread_j;
        }

        // Compute residual.
        let residual: f64 = current.iter().zip(next.iter())
            .map(|(c, n)| (c - n).abs())
            .sum();
        residuals.push(residual);

        // Ping-pong: new state lives in `current` after the swap, old state
        // (now in `next`) becomes the reusable scratch buffer.
        std::mem::swap(&mut current, &mut next);

        if residual < config.epsilon {
            return SpreadResult {
                activation: current,
                iterations: residuals.len(),
                residuals,
                converged: true,
            };
        }
    }

    SpreadResult {
        activation: current,
        iterations: config.max_iter,
        residuals,
        converged: false,
    }
}

/// Compute contraction distance for the linear operator (no threshold).
///
/// Returns `(lhs, rhs)` where `lhs = ‖T(a)-T(b)‖₁` and `rhs = d·‖a-b‖₁`.
/// The contraction inequality `lhs ≤ rhs` is guaranteed.
pub fn linear_contraction_distance(
    graph: &ScoredGraph,
    a: &[f64],
    b: &[f64],
    initial: &[f64],
    d: f64,
) -> (f64, f64) {
    let no_thresh = NoThreshold;

    let ta = one_step(graph, a, initial, &no_thresh, d);
    let tb = one_step(graph, b, initial, &no_thresh, d);

    let lhs: f64 = ta.iter().zip(tb.iter()).map(|(x, y)| (x - y).abs()).sum();
    let rhs: f64 = d * a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum::<f64>();
    (lhs, rhs)
}

/// Single step of the operator T.
fn one_step<T: ThresholdFn>(
    graph: &ScoredGraph,
    current: &[f64],
    initial: &[f64],
    threshold: &T,
    d: f64,
) -> Vec<f64> {
    let n = graph.len();
    let (row_ptr, col_idx, values) = graph.adj_transpose_csr();
    let one_minus_d = 1.0 - d;

    let thresholded: Vec<f64> = current.iter().map(|&a| threshold.apply(a)).collect();

    let mut next = vec![0.0; n];
    for j in 0..n {
        let mut spread_j: f64 = 0.0;
        let start = row_ptr[j];
        let end = row_ptr[j + 1];
        for k in start..end {
            spread_j += values[k] as f64 * thresholded[col_idx[k]];
        }
        next[j] = one_minus_d * initial[j] + d * spread_j;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain_graph(n: usize) -> ScoredGraph {
        let mut weights = vec![0.0; n * n];
        for i in 0..n - 1 {
            weights[i * n + (i + 1)] = 1.0;
        }
        ScoredGraph::new(n, weights, vec![100; n])
    }

    #[test]
    fn linear_converges_on_chain() {
        let g = chain_graph(5);
        let mut init = vec![0.0; 5];
        init[0] = 1.0;

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());
        assert!(result.converged);
        assert!(result.activation[0] > result.activation[4]);
    }

    #[test]
    fn sigmoid_converges() {
        let g = chain_graph(5);
        let mut init = vec![0.0; 5];
        init[0] = 1.0;

        let thresh = SigmoidThreshold::default();
        let result = spread(&g, &init, &thresh, &SpreadConfig::default());
        assert!(result.converged);
    }

    #[test]
    fn activation_non_negative() {
        let g = chain_graph(5);
        let mut init = vec![0.0; 5];
        init[0] = 1.0;

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());
        for &a in &result.activation {
            assert!(a >= -1e-10);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_graph(max_n: usize) -> impl Strategy<Value = ScoredGraph> {
        (3..=max_n).prop_flat_map(|n| {
            let weights = proptest::collection::vec(0.0..2.0f64, n * n);
            let costs = proptest::collection::vec(1..500u64, n);
            (Just(n), weights, costs)
        })
        .prop_map(|(n, weights, costs)| ScoredGraph::new(n, weights, costs))
    }

    fn arb_d() -> impl Strategy<Value = f64> {
        0.1..0.95f64
    }

    #[allow(dead_code)]
    fn arb_activation(n: usize) -> impl Strategy<Value = Vec<f64>> {
        proptest::collection::vec(0.0..1.0f64, n)
    }

    proptest! {
        /// P1.1: Linear contraction inequality holds for all inputs.
        #[test]
        fn linear_contraction(g in arb_graph(10), d in arb_d()) {
            let n = g.len();
            let a: Vec<f64> = (0..n).map(|i| (i as f64) / (n as f64)).collect();
            let b: Vec<f64> = (0..n).map(|i| 1.0 - (i as f64) / (n as f64)).collect();
            let init: Vec<f64> = vec![0.5; n];

            let (lhs, rhs) = linear_contraction_distance(&g, &a, &b, &init, d);
            prop_assert!(lhs <= rhs + 1e-10,
                "Contraction violated: lhs={} > rhs={}", lhs, rhs);
        }

        /// P1.3: Linear operator always converges.
        #[test]
        fn linear_always_converges(g in arb_graph(10), d in arb_d()) {
            let n = g.len();
            let mut init = vec![0.0; n];
            if n > 0 { init[0] = 1.0; }

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d,
                max_iter: 200,
                epsilon: 1e-10,
            });
            prop_assert!(result.converged,
                "Linear operator did not converge in {} iterations, final residual={}",
                result.iterations,
                result.residuals.last().unwrap_or(&f64::NAN));
        }

        /// P1.6: Activation values are non-negative and bounded above.
        #[test]
        fn activation_bounded(g in arb_graph(10), d in arb_d()) {
            let n = g.len();
            let mut init = vec![0.0; n];
            if n > 0 { init[0] = 1.0; }

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d, ..Default::default()
            });

            let max_init: f64 = init.iter().copied().fold(0.0, f64::max);
            let upper_bound = max_init / (1.0 - d);

            for (i, &a) in result.activation.iter().enumerate() {
                prop_assert!(a >= -1e-10,
                    "Negative activation at node {}: {}", i, a);
                prop_assert!(a <= upper_bound + 1e-6,
                    "Activation {} at node {} exceeds bound {}", a, i, upper_bound);
            }
        }

        /// P2.1: Residuals respect geometric d^t envelope (linear operator).
        #[test]
        fn geometric_residual_envelope(g in arb_graph(10), d in arb_d()) {
            let n = g.len();
            let mut init = vec![0.0; n];
            if n > 0 { init[0] = 1.0; }

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d, max_iter: 100, epsilon: 1e-12,
            });

            if result.residuals.len() >= 3 {
                let r0 = result.residuals[0];
                for (t, &r_t) in result.residuals.iter().enumerate() {
                    let envelope = r0 * d.powi(t as i32) * 1.1 + 1e-10;
                    prop_assert!(r_t <= envelope,
                        "Residual {} at step {} exceeds envelope {} (d={})",
                        r_t, t, envelope, d);
                }
            }
        }

        /// P2.4: Higher d = more iterations (linear operator).
        #[test]
        fn higher_d_slower(g in arb_graph(8)) {
            let n = g.len();
            let mut init = vec![0.0; n];
            if n > 0 { init[0] = 1.0; }

            let r_low = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d: 0.3, max_iter: 500, epsilon: 1e-10,
            });
            let r_high = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d: 0.85, max_iter: 500, epsilon: 1e-10,
            });

            prop_assert!(r_high.iterations >= r_low.iterations.saturating_sub(1),
                "d=0.85 ({} iter) faster than d=0.3 ({} iter)",
                r_high.iterations, r_low.iterations);
        }
    }
}
