//! VOPR-style deterministic chaos simulation.
//!
//! Seeded PRNG drives a state machine that randomly interleaves:
//! - Spreading activation queries
//! - REM maintenance cycles
//! - Graph mutations (add/remove edges, corrupt weights)
//! - Invariant checks at every step
//!
//! If any invariant breaks, the seed is the reproducer.

use scored_graph::ScoredGraph;
use spread::{spread, NoThreshold, SigmoidThreshold, SpreadConfig};
use temporal_graph::{TemporalState, RemConfig, rem_cycle};
use budget_knap::{select, Item};

/// Minimal seeded PRNG (xorshift64). Deterministic, reproducible.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn next_usize(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max
    }

    #[allow(dead_code)]
    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }
}

/// What the simulator does at each step.
#[derive(Debug, Clone, Copy)]
enum Action {
    /// Run spreading activation from a random seed node.
    SpreadQuery,
    /// Run a full REM maintenance cycle.
    RemCycle,
    /// Add a random edge.
    AddEdge,
    /// Zero out a random edge (remove).
    RemoveEdge,
    /// Corrupt a random weight to an extreme value.
    CorruptWeight,
    /// Check all invariants.
    CheckInvariants,
}

fn random_action(rng: &mut Rng) -> Action {
    match rng.next_usize(6) {
        0 => Action::SpreadQuery,
        1 => Action::RemCycle,
        2 => Action::AddEdge,
        3 => Action::RemoveEdge,
        4 => Action::CorruptWeight,
        _ => Action::CheckInvariants,
    }
}

/// Generate a random graph of size n.
fn random_graph(rng: &mut Rng, n: usize, edge_prob: f64) -> ScoredGraph {
    let mut weights = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            if i != j && rng.next_f64() < edge_prob {
                weights[i * n + j] = rng.next_f64() * 2.0;
            }
        }
    }
    let costs: Vec<u64> = (0..n).map(|_| (rng.next_usize(400) + 10) as u64).collect();
    ScoredGraph::new(n, weights, costs)
}

/// Check every invariant we've proven. Returns an error string if anything breaks.
fn check_invariants(graph: &ScoredGraph, label: &str) -> Result<(), String> {
    // 1. Row-stochastic.
    if !graph.is_row_stochastic() {
        return Err(format!("[{label}] Row-stochastic invariant violated"));
    }

    // 2. No self-loops.
    for i in 0..graph.len() {
        if graph.raw_weight(i, i) != 0.0 {
            return Err(format!("[{label}] Self-loop at node {i}"));
        }
    }

    // 3. Non-negative weights.
    for i in 0..graph.len() {
        for j in 0..graph.len() {
            if graph.raw_weight(i, j) < 0.0 {
                return Err(format!(
                    "[{label}] Negative weight at ({i},{j}): {}",
                    graph.raw_weight(i, j)
                ));
            }
        }
    }

    Ok(())
}

/// Check spread result invariants.
fn check_spread_invariants(
    activation: &[f64],
    initial: &[f64],
    d: f64,
    label: &str,
) -> Result<(), String> {
    let max_init = initial.iter().copied().fold(0.0f64, f64::max);
    let upper_bound = if d < 1.0 { max_init / (1.0 - d) } else { f64::MAX };

    for (i, &a) in activation.iter().enumerate() {
        if a < -1e-9 {
            return Err(format!("[{label}] Negative activation at {i}: {a}"));
        }
        if a > upper_bound + 1e-3 {
            return Err(format!(
                "[{label}] Activation {a} at node {i} exceeds bound {upper_bound}"
            ));
        }
        if a.is_nan() {
            return Err(format!("[{label}] NaN activation at node {i}"));
        }
        if a.is_infinite() {
            return Err(format!("[{label}] Infinite activation at node {i}"));
        }
    }

    Ok(())
}

/// Check budget-knap invariants.
fn check_knapsack_invariants(
    items: &[Item],
    budget: u64,
    label: &str,
) -> Result<(), String> {
    let sel = select(items, budget);

    // Budget never exceeded.
    if sel.total_cost > budget {
        return Err(format!(
            "[{label}] Budget violated: used {}, budget={budget}",
            sel.total_cost
        ));
    }

    // Score is non-negative.
    if sel.total_score < -1e-9 {
        return Err(format!("[{label}] Negative total score: {}", sel.total_score));
    }

    // Score matches sum of selected items.
    let expected_score: f64 = sel.indices.iter().map(|&i| items[i].score).sum();
    if (sel.total_score - expected_score).abs() > 1e-9 {
        return Err(format!(
            "[{label}] Score mismatch: reported={}, computed={expected_score}",
            sel.total_score
        ));
    }

    Ok(())
}

/// Run the full VOPR simulation for `n_steps` using `seed`.
///
/// Returns `Ok(())` if all invariants held, or `Err(message)` with
/// the step number and seed for reproduction.
pub fn vopr_run(seed: u64, n: usize, n_steps: usize) -> Result<(), String> {
    let mut rng = Rng::new(seed);
    let mut graph = random_graph(&mut rng, n, 0.3);
    let mut state = TemporalState::new(n);
    let config = RemConfig {
        decay_rate: 0.05,
        prune_threshold: 0.01,
        prune_window: 5,
        d: 0.8,
        activation_threshold: 0.01,
    };

    for step in 0..n_steps {
        let label = format!("seed={seed:#x} step={step}");
        let action = random_action(&mut rng);

        match action {
            Action::SpreadQuery => {
                let mut init = vec![0.0; n];
                let seed_node = rng.next_usize(n);
                init[seed_node] = rng.next_f64().max(0.1);

                let d = 0.1 + rng.next_f64() * 0.85;
                let result = spread(
                    &graph,
                    &init,
                    &SigmoidThreshold::default(),
                    &SpreadConfig { d, max_iter: 100, epsilon: 1e-8 },
                );

                check_spread_invariants(&result.activation, &init, d, &label)?;
            }

            Action::RemCycle => {
                let mut init = vec![0.0; n];
                let alive: Vec<usize> = (0..n).filter(|&v| state.alive[v]).collect();
                if !alive.is_empty() {
                    let seed_node = alive[rng.next_usize(alive.len())];
                    init[seed_node] = 1.0;
                }

                rem_cycle(
                    &mut graph,
                    &mut state,
                    &init,
                    &config,
                    None::<fn(usize, usize) -> f64>,
                );

                check_invariants(&graph, &label)?;
            }

            Action::AddEdge => {
                let src = rng.next_usize(n);
                let dst = rng.next_usize(n);
                if src != dst {
                    graph.set_edge(src, dst, (rng.next_f64() * 2.0) as f32);
                    graph.renormalize();
                }
                check_invariants(&graph, &label)?;
            }

            Action::RemoveEdge => {
                let src = rng.next_usize(n);
                let dst = rng.next_usize(n);
                if src != dst {
                    graph.set_edge(src, dst, 0.0);
                    graph.renormalize();
                }
                check_invariants(&graph, &label)?;
            }

            Action::CorruptWeight => {
                let src = rng.next_usize(n);
                let dst = rng.next_usize(n);
                if src != dst {
                    let extreme = match rng.next_usize(4) {
                        0 => f64::MIN_POSITIVE,       // denormalized
                        1 => 1e-15,                    // near-zero
                        2 => 1e6,                      // very large
                        _ => rng.next_f64() * 100.0,   // moderately large
                    };
                    // f32 floor: values below f32::MIN_POSITIVE underflow to 0.
                    let w = extreme as f32;
                    graph.set_edge(src, dst, w);
                    graph.renormalize();
                }
                check_invariants(&graph, &label)?;
            }

            Action::CheckInvariants => {
                check_invariants(&graph, &label)?;

                // Also check knapsack with current state.
                let init = vec![0.0; n];
                let result = spread(
                    &graph,
                    &init,
                    &NoThreshold,
                    &SpreadConfig::default(),
                );
                let items: Vec<Item> = result.activation.iter()
                    .enumerate()
                    .map(|(i, &s)| Item { score: s, cost: graph.cost(i) })
                    .collect();
                let budget = (rng.next_usize(2000) + 50) as u64;
                check_knapsack_invariants(&items, budget, &label)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run VOPR across many seeds.
    #[test]
    fn vopr_100_seeds() {
        for seed in 0..100u64 {
            vopr_run(seed, 10, 200).unwrap_or_else(|e| {
                panic!("VOPR failed: {e}");
            });
        }
    }

    /// Small graph, many steps — stress state accumulation.
    #[test]
    fn vopr_small_long() {
        vopr_run(0xCAFE, 4, 1000).unwrap_or_else(|e| {
            panic!("VOPR small-long failed: {e}");
        });
    }

    /// Large graph, fewer steps — stress matrix operations.
    #[test]
    fn vopr_large_short() {
        vopr_run(0xBEEF, 50, 100).unwrap_or_else(|e| {
            panic!("VOPR large-short failed: {e}");
        });
    }

    /// Extreme seed values.
    #[test]
    fn vopr_extreme_seeds() {
        for seed in [u64::MAX, u64::MAX - 1, 1, 0xDEADBEEF, 0x1337] {
            vopr_run(seed, 12, 300).unwrap_or_else(|e| {
                panic!("VOPR failed on seed {seed:#x}: {e}");
            });
        }
    }
}
