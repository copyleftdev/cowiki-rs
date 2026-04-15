//! # temporal-graph
//!
//! Temporal graph dynamics: decay, prune, and dream operators.
//!
//! Formalizes the REM Agent's maintenance cycle on a scored graph:
//!
//! ```text
//! Gₜ → Gₜ₊₁ via:
//!   Decay:  w_t(i,j) = w₀(i,j) · exp(-λ · r(vᵢ, t))
//!   Prune:  remove v if max activation over window < θ
//!   Dream:  add (u,v) if sim(u,v) > θ and (u,v) ∉ E
//! ```

use scored_graph::ScoredGraph;
use spread::{spread, SpreadConfig, NoThreshold};

/// Temporal state tracked across time steps.
#[derive(Debug, Clone)]
pub struct TemporalState {
    /// Current time step.
    pub time: u64,
    /// Last access time per node.
    pub last_access: Vec<u64>,
    /// Activation history (one vector per time step).
    pub activation_history: Vec<Vec<f64>>,
    /// Health metric at each step.
    pub health_history: Vec<f64>,
    /// Whether each node is still alive (not pruned).
    pub alive: Vec<bool>,
}

impl TemporalState {
    /// Initialize temporal state for a graph of size `n`.
    pub fn new(n: usize) -> Self {
        Self {
            time: 0,
            last_access: vec![0; n],
            activation_history: Vec::new(),
            health_history: Vec::new(),
            alive: vec![true; n],
        }
    }

    /// Number of alive nodes.
    pub fn alive_count(&self) -> usize {
        self.alive.iter().filter(|&&a| a).count()
    }

    /// Access recency for node `v`: `time - last_access[v]`.
    pub fn recency(&self, v: usize) -> u64 {
        self.time.saturating_sub(self.last_access[v])
    }
}

/// Configuration for the REM cycle.
#[derive(Debug, Clone)]
pub struct RemConfig {
    /// Exponential decay rate λ.
    pub decay_rate: f64,
    /// Prune threshold: nodes with max activation below this get pruned.
    pub prune_threshold: f64,
    /// Number of past steps to consider for pruning.
    pub prune_window: usize,
    /// Spreading activation propagation factor.
    pub d: f64,
    /// Spreading activation threshold.
    pub activation_threshold: f64,
}

impl Default for RemConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.05,
            prune_threshold: 0.001,
            prune_window: 5,
            d: 0.8,
            activation_threshold: 0.01,
        }
    }
}

/// Report from a REM cycle.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Graph health H(Gₜ) ∈ [0, 1].
    pub health: f64,
    /// Nodes pruned this cycle.
    pub pruned: Vec<usize>,
    /// New edges discovered by dreaming.
    pub dreamed_edges: Vec<(usize, usize)>,
}

// ─── Operators ───────────────────────────────────────────────────────────────

/// Apply exponential decay to edge weights based on source-node recency.
///
/// ```text
/// w_t(i,j) = w_raw(i,j) · exp(-λ · r(vᵢ, t))
/// ```
///
/// ## Proven properties (P6.1–P6.2)
/// - Decay monotonically increases with access recency
/// - Follows exact exponential formula
pub fn decay(graph: &mut ScoredGraph, state: &TemporalState, decay_rate: f64) {
    let n = graph.len();
    let raw = graph.raw_matrix_mut();

    for i in 0..n {
        let r = state.recency(i) as f64;
        let factor = (-decay_rate * r).exp();
        for j in 0..n {
            raw[i * n + j] *= factor;
        }
    }

    graph.renormalize();
}

/// Identify nodes to prune: those whose max activation over the last
/// `window` steps never exceeded `threshold`.
///
/// ## Proven properties (P6.3–P6.4)
/// - Active nodes (activation > threshold) are never pruned
/// - Dormant nodes (zero activation over window) are always prunable
pub fn prune_candidates(state: &TemporalState, threshold: f64, window: usize) -> Vec<usize> {
    if state.activation_history.len() < window {
        return vec![];
    }

    let recent = &state.activation_history[state.activation_history.len() - window..];
    let n = state.alive.len();
    let mut prunable = Vec::new();

    for v in 0..n {
        if !state.alive[v] {
            continue;
        }
        let max_act = recent.iter()
            .map(|a| if v < a.len() { a[v] } else { 0.0 })
            .fold(0.0f64, f64::max);
        if max_act < threshold {
            prunable.push(v);
        }
    }

    prunable
}

/// Discover new edges between nodes whose similarity exceeds `threshold`
/// but have no existing edge.
///
/// The `similarity` closure is called for each candidate pair and should
/// return a score in [0, 1].
///
/// ## Proven properties (P6.5–P6.6)
/// - Discovers edges between similar unconnected nodes
/// - Never proposes duplicate edges
pub fn dream_candidates<F>(
    graph: &ScoredGraph,
    state: &TemporalState,
    threshold: f64,
    similarity: F,
) -> Vec<(usize, usize)>
where
    F: Fn(usize, usize) -> f64,
{
    let n = graph.len();
    let mut new_edges = Vec::new();

    for i in 0..n {
        if !state.alive[i] { continue; }
        for j in 0..n {
            if i == j || !state.alive[j] { continue; }
            if graph.raw_weight(i, j) > 0.0 { continue; }
            if similarity(i, j) > threshold {
                new_edges.push((i, j));
            }
        }
    }

    new_edges
}

/// Compute graph health: fraction of alive nodes reachable from at least
/// one random probe query.
///
/// ## Proven property (P6.7)
/// Health stays > 0 under gentle REM parameters.
pub fn graph_health(graph: &ScoredGraph, state: &TemporalState, config: &RemConfig) -> f64 {
    let alive_count = state.alive_count();
    if alive_count == 0 {
        return 0.0;
    }

    let n = graph.len();
    let alive_nodes: Vec<usize> = (0..n).filter(|&v| state.alive[v]).collect();
    let n_probes = alive_nodes.len().min(10);
    let mut ever_activated = vec![false; n];

    let spread_cfg = SpreadConfig {
        d: config.d,
        max_iter: 50,
        epsilon: 1e-8,
    };

    for &seed in alive_nodes.iter().take(n_probes) {
        let mut init = vec![0.0; n];
        init[seed] = 1.0;

        let result = spread(graph, &init, &NoThreshold, &spread_cfg);

        for (v, &a) in result.activation.iter().enumerate() {
            if a > config.activation_threshold && state.alive[v] {
                ever_activated[v] = true;
            }
        }
    }

    let reachable = ever_activated.iter().filter(|&&x| x).count();
    reachable as f64 / alive_count as f64
}

/// Execute one full REM cycle: activate → decay → prune → dream → health.
pub fn rem_cycle<F>(
    graph: &mut ScoredGraph,
    state: &mut TemporalState,
    query_activation: &[f64],
    config: &RemConfig,
    similarity: Option<F>,
) -> HealthReport
where
    F: Fn(usize, usize) -> f64,
{
    let n = graph.len();
    state.time += 1;

    // 1. Run spreading activation for this query.
    let spread_cfg = SpreadConfig {
        d: config.d,
        max_iter: 100,
        epsilon: 1e-8,
    };
    let result = spread(graph, query_activation, &NoThreshold, &spread_cfg);
    state.activation_history.push(result.activation.clone());

    // 2. Update access times.
    for (v, &a) in result.activation.iter().enumerate() {
        if a > config.activation_threshold {
            state.last_access[v] = state.time;
        }
    }

    // 3. Decay.
    decay(graph, state, config.decay_rate);

    // 4. Prune.
    let pruned = prune_candidates(state, config.prune_threshold, config.prune_window);
    for &v in &pruned {
        state.alive[v] = false;
    }

    // 5. Dream.
    let dreamed_edges = if let Some(sim) = similarity {
        let edges = dream_candidates(graph, state, 0.5, sim);
        let raw = graph.raw_matrix_mut();
        for &(src, dst) in &edges {
            raw[src * n + dst] = 0.5;
        }
        if !edges.is_empty() {
            graph.renormalize();
        }
        edges
    } else {
        vec![]
    };

    // 6. Health.
    let health = graph_health(graph, state, config);
    state.health_history.push(health);

    HealthReport { health, pruned, dreamed_edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_graph() -> ScoredGraph {
        ScoredGraph::new(
            4,
            vec![
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
                0.0, 0.0, 0.0, 0.0,
            ],
            vec![100; 4],
        )
    }

    #[test]
    fn decay_reduces_weights() {
        let mut g = simple_graph();
        let mut state = TemporalState::new(4);
        state.time = 10;
        state.last_access = vec![9, 2, 2, 2]; // node 0 recent, others stale

        let orig_01 = g.raw_weight(0, 1);
        let orig_12 = g.raw_weight(1, 2);

        decay(&mut g, &state, 0.1);

        // Node 0 (recency=1) should decay less than node 1 (recency=8).
        let ratio_0 = g.raw_weight(0, 1) / orig_01;
        let ratio_1 = g.raw_weight(1, 2) / orig_12;
        assert!(ratio_0 > ratio_1, "Recent node decayed more than stale");
    }

    #[test]
    fn active_nodes_not_pruned() {
        let state = TemporalState {
            time: 5,
            last_access: vec![5; 4],
            activation_history: (0..5).map(|_| vec![0.5, 0.0, 0.0, 0.0]).collect(),
            health_history: vec![],
            alive: vec![true; 4],
        };

        let prunable = prune_candidates(&state, 0.01, 5);
        assert!(!prunable.contains(&0), "Active node should not be prunable");
    }

    #[test]
    fn dormant_nodes_pruned() {
        let state = TemporalState {
            time: 5,
            last_access: vec![5; 4],
            activation_history: (0..5).map(|_| vec![0.5, 0.0, 0.0, 0.0]).collect(),
            health_history: vec![],
            alive: vec![true; 4],
        };

        let prunable = prune_candidates(&state, 0.01, 5);
        for v in 1..4 {
            assert!(prunable.contains(&v), "Dormant node {v} should be prunable");
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_graph(max_n: usize) -> impl Strategy<Value = ScoredGraph> {
        (4..=max_n).prop_flat_map(|n| {
            let weights = proptest::collection::vec(0.0..2.0f64, n * n);
            let costs = proptest::collection::vec(1..500u64, n);
            (Just(n), weights, costs)
        })
        .prop_map(|(n, weights, costs)| ScoredGraph::new(n, weights, costs))
    }

    proptest! {
        /// P6.2: Decay follows exact exponential formula.
        #[test]
        fn decay_is_exponential(
            g in arb_graph(8),
            decay_rate in 0.01..0.5f64,
        ) {
            let mut g = g;
            let n = g.len();

            let mut state = TemporalState::new(n);
            state.time = 20;
            for i in 0..n {
                state.last_access[i] = 20u64.saturating_sub(i as u64 + 1);
            }

            // Snapshot original weights.
            let orig: Vec<f64> = g.raw_matrix().to_vec();

            decay(&mut g, &state, decay_rate);

            for i in 0..n {
                let r = state.recency(i) as f64;
                let factor = (-decay_rate * r).exp();
                for j in 0..n {
                    let expected = orig[i * n + j] * factor;
                    let actual = g.raw_weight(i, j);
                    prop_assert!((actual - expected).abs() < 1e-9,
                        "Decay mismatch at ({i},{j}): got {actual}, expected {expected}");
                }
            }
        }

        /// P6.1: Recent nodes decay less than stale nodes.
        #[test]
        fn decay_monotonic_with_recency(
            g in arb_graph(6),
            decay_rate in 0.01..0.5f64,
        ) {
            let mut g = g;
            let n = g.len();
            if n < 2 { return Ok(()); }

            let mut state = TemporalState::new(n);
            state.time = 10;
            state.last_access[0] = 9;  // recency = 1
            state.last_access[1] = 2;  // recency = 8

            let orig: Vec<f64> = g.raw_matrix().to_vec();

            decay(&mut g, &state, decay_rate);

            for j in 0..n {
                if orig[j] > 0.0 && orig[n + j] > 0.0 {
                    let ratio_0 = g.raw_weight(0, j) / orig[j];
                    let ratio_1 = g.raw_weight(1, j) / orig[n + j];
                    prop_assert!(ratio_0 >= ratio_1 - 1e-9,
                        "Recent node decayed more: {ratio_0} < {ratio_1}");
                }
            }
        }

        /// P6.5: Dream doesn't propose duplicate edges.
        #[test]
        fn dream_no_duplicates(g in arb_graph(8)) {
            let state = TemporalState::new(g.len());

            let edges = dream_candidates(&g, &state, 0.0, |_, _| 1.0);

            for &(src, dst) in &edges {
                let w = g.raw_weight(src, dst);
                prop_assert_eq!(w, 0.0,
                    "Dream proposed duplicate edge ({},{})", src, dst);
            }
        }
    }
}
