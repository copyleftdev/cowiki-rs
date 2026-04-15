//! Handcrafted adversarial topologies designed to break spreading activation.
//!
//! Each topology targets a specific failure mode or edge case.

#[cfg(test)]
mod tests {
    use scored_graph::ScoredGraph;
    use spread::{spread, NoThreshold, SigmoidThreshold, SpreadConfig};
    use budget_knap::{select, Item};

    /// Single-node graph. The degenerate case.
    fn single_node() -> ScoredGraph {
        ScoredGraph::new(1, vec![0.0], vec![100])
    }

    /// Two nodes, no edges. Completely disconnected.
    fn disconnected_pair() -> ScoredGraph {
        ScoredGraph::new(2, vec![0.0; 4], vec![100, 100])
    }

    /// Complete graph: every node links to every other.
    fn complete(n: usize) -> ScoredGraph {
        let mut w = vec![1.0; n * n];
        for i in 0..n {
            w[i * n + i] = 0.0; // no self-loops
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Star graph: node 0 links to all others, no other edges.
    fn star(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        // Row 0: hub links to all spokes.
        for slot in w.iter_mut().take(n).skip(1) {
            *slot = 1.0;
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Reverse star: all nodes link to node 0.
    fn reverse_star(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in 1..n {
            w[i * n] = 1.0; // spokes → hub
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Long chain: 0 → 1 → 2 → ... → n-1.
    fn chain(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in 0..n - 1 {
            w[i * n + (i + 1)] = 1.0;
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Cycle: 0 → 1 → 2 → ... → n-1 → 0.
    fn cycle(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in 0..n {
            w[i * n + ((i + 1) % n)] = 1.0;
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Bipartite graph: two groups, edges only between groups.
    fn bipartite(n: usize) -> ScoredGraph {
        let half = n / 2;
        let mut w = vec![0.0; n * n];
        for i in 0..half {
            for j in half..n {
                w[i * n + j] = 1.0;
                w[j * n + i] = 1.0;
            }
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Barbell: two complete subgraphs connected by a single edge.
    fn barbell(half: usize) -> ScoredGraph {
        let n = half * 2;
        let mut w = vec![0.0; n * n];
        // Left clique.
        for i in 0..half {
            for j in 0..half {
                if i != j { w[i * n + j] = 1.0; }
            }
        }
        // Right clique.
        for i in half..n {
            for j in half..n {
                if i != j { w[i * n + j] = 1.0; }
            }
        }
        // Bridge.
        w[(half - 1) * n + half] = 0.1; // weak link
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// All topologies, named for error messages.
    fn all_topologies() -> Vec<(&'static str, ScoredGraph)> {
        vec![
            ("single_node",      single_node()),
            ("disconnected_pair", disconnected_pair()),
            ("complete_5",       complete(5)),
            ("complete_20",      complete(20)),
            ("star_10",          star(10)),
            ("reverse_star_10",  reverse_star(10)),
            ("chain_10",         chain(10)),
            ("chain_50",         chain(50)),
            ("cycle_10",         cycle(10)),
            ("cycle_50",         cycle(50)),
            ("bipartite_10",     bipartite(10)),
            ("barbell_5",        barbell(5)),
            ("barbell_10",       barbell(10)),
        ]
    }

    /// Every topology must produce a valid row-stochastic graph.
    #[test]
    fn all_topologies_valid() {
        for (name, g) in all_topologies() {
            assert!(g.is_row_stochastic(),
                "{name}: row-stochastic invariant violated");
            for i in 0..g.len() {
                assert_eq!(g.raw_weight(i, i), 0.0,
                    "{name}: self-loop at {i}");
            }
        }
    }

    /// Spreading activation must converge on every topology (linear operator).
    #[test]
    fn linear_converges_on_all_topologies() {
        for (name, g) in all_topologies() {
            let n = g.len();
            let mut init = vec![0.0; n];
            init[0] = 1.0;

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d: 0.8,
                max_iter: 500,
                epsilon: 1e-10,
            });

            assert!(result.converged,
                "{name}: linear operator did not converge in {} iterations, \
                 final residual={}",
                result.iterations,
                result.residuals.last().unwrap_or(&f64::NAN));
        }
    }

    /// Sigmoid threshold must also converge on every topology.
    #[test]
    fn sigmoid_converges_on_all_topologies() {
        for (name, g) in all_topologies() {
            let n = g.len();
            let mut init = vec![0.0; n];
            init[0] = 1.0;

            let result = spread(&g, &init, &SigmoidThreshold::default(), &SpreadConfig {
                d: 0.8,
                max_iter: 500,
                epsilon: 1e-10,
            });

            assert!(result.converged,
                "{name}: sigmoid did not converge in {} iterations",
                result.iterations);
        }
    }

    /// No topology should produce NaN or infinite activation.
    #[test]
    fn no_nan_or_inf_on_any_topology() {
        for (name, g) in all_topologies() {
            let n = g.len();
            // Try activating every node as the seed.
            for seed_node in 0..n {
                let mut init = vec![0.0; n];
                init[seed_node] = 1.0;

                let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

                for (i, &a) in result.activation.iter().enumerate() {
                    assert!(!a.is_nan(),
                        "{name}: NaN at node {i} (seed={seed_node})");
                    assert!(!a.is_infinite(),
                        "{name}: Inf at node {i} (seed={seed_node})");
                    assert!(a >= -1e-10,
                        "{name}: negative activation {a} at node {i}");
                }
            }
        }
    }

    /// On a disconnected graph, activation must not leak between components.
    #[test]
    fn disconnected_no_leakage() {
        let g = disconnected_pair();
        let init = vec![1.0, 0.0];

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

        // Node 1 was never activated and has no incoming edges.
        assert_eq!(result.activation[1], 0.0,
            "Activation leaked to disconnected node: {}",
            result.activation[1]);
    }

    /// On a star graph, spokes should have equal activation.
    #[test]
    fn star_spokes_symmetric() {
        let g = star(10);
        let mut init = vec![0.0; 10];
        init[0] = 1.0;

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
            d: 0.8, ..Default::default()
        });

        let spoke_activations: Vec<f64> = result.activation[1..].to_vec();
        let first = spoke_activations[0];
        for (i, &a) in spoke_activations.iter().enumerate() {
            assert!((a - first).abs() < 1e-9,
                "Star spoke asymmetry: spoke 1={first}, spoke {}={a}",
                i + 1);
        }
    }

    /// On a cycle, all nodes should reach equal activation when all are seeded.
    #[test]
    fn cycle_uniform_activation() {
        let n = 10;
        let g = cycle(n);
        let init = vec![1.0 / n as f64; n]; // uniform seed

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
            d: 0.8, ..Default::default()
        });

        let first = result.activation[0];
        for (i, &a) in result.activation.iter().enumerate() {
            assert!((a - first).abs() < 1e-6,
                "Cycle asymmetry: node 0={first}, node {i}={a}");
        }
    }

    /// Barbell: activation in the far clique should be much less than the near clique.
    #[test]
    fn barbell_activation_bottleneck() {
        let g = barbell(5);
        let mut init = vec![0.0; 10];
        init[0] = 1.0; // Seed in left clique.

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
            d: 0.8, ..Default::default()
        });

        let left_total: f64 = result.activation[..5].iter().sum();
        let right_total: f64 = result.activation[5..].iter().sum();

        assert!(left_total > right_total,
            "Barbell: left clique ({left_total:.4}) should have more \
             activation than right ({right_total:.4}) with weak bridge");
    }

    /// Complete graph: spreading activation must not blow up.
    #[test]
    fn complete_graph_bounded() {
        let g = complete(20);
        let mut init = vec![0.0; 20];
        init[0] = 1.0;

        for d in [0.1, 0.5, 0.8, 0.95, 0.99] {
            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d, max_iter: 500, epsilon: 1e-10,
            });

            let max_a = result.activation.iter().copied().fold(0.0f64, f64::max);
            let bound = 1.0 / (1.0 - d);
            assert!(max_a <= bound + 1e-3,
                "Complete graph with d={d}: max activation {max_a} > bound {bound}");
        }
    }

    /// Knapsack on every topology: budget constraint must hold.
    #[test]
    fn knapsack_budget_on_all_topologies() {
        for (name, g) in all_topologies() {
            let n = g.len();
            let mut init = vec![0.0; n];
            if n > 0 { init[0] = 1.0; }

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

            let items: Vec<Item> = result.activation.iter()
                .enumerate()
                .map(|(i, &s)| Item { score: s, cost: g.cost(i) })
                .collect();

            for budget in [1, 50, 100, 500, 10000] {
                let sel = select(&items, budget);
                assert!(sel.total_cost <= budget,
                    "{name}: budget violated at B={budget}");
            }
        }
    }
}
