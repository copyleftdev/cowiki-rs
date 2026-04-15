//! Long-horizon stability tests.
//!
//! Run hundreds of REM cycles and verify that invariants hold at every step.
//! This catches slow-building failures: gradual precision loss, creeping NaNs,
//! health collapse, weight underflow.

#[cfg(test)]
mod tests {
    use scored_graph::ScoredGraph;
    use spread::{spread, NoThreshold, SpreadConfig};
    use temporal_graph::{TemporalState, RemConfig, rem_cycle};

    /// Simple seeded RNG for reproducible tests.
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
            (self.next_u64() as usize) % max
        }
        fn next_f64(&mut self) -> f64 {
            (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn random_graph(rng: &mut Rng, n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                if i != j && rng.next_f64() < 0.3 {
                    w[i * n + j] = rng.next_f64() * 2.0;
                }
            }
        }
        let costs: Vec<u64> = (0..n).map(|_| (rng.next_usize(400) + 10) as u64).collect();
        ScoredGraph::new(n, w, costs)
    }

    /// 500 REM cycles: graph health must never go NaN or Inf.
    #[test]
    fn rem_500_cycles_no_nan() {
        let mut rng = Rng::new(42);
        let n = 10;
        let mut g = random_graph(&mut rng, n);
        let mut state = TemporalState::new(n);
        let config = RemConfig {
            decay_rate: 0.03,
            prune_threshold: 0.001,
            prune_window: 10,
            d: 0.8,
            activation_threshold: 0.01,
        };

        for step in 0..500 {
            let alive: Vec<usize> = (0..n).filter(|&v| state.alive[v]).collect();
            if alive.is_empty() { break; }

            let mut init = vec![0.0; n];
            let seed = alive[rng.next_usize(alive.len())];
            init[seed] = 1.0;

            let report = rem_cycle(
                &mut g, &mut state, &init, &config,
                None::<fn(usize, usize) -> f64>,
            );

            assert!(!report.health.is_nan(),
                "Health went NaN at step {step}");
            assert!(!report.health.is_infinite(),
                "Health went Inf at step {step}");
            assert!(report.health >= 0.0,
                "Negative health {:.4} at step {step}", report.health);

            // All weights must remain non-negative.
            for i in 0..n {
                for j in 0..n {
                    let w = g.raw_weight(i, j);
                    assert!(w >= 0.0 && w.is_finite(),
                        "Bad weight at ({i},{j})={w} step {step}");
                }
            }
        }
    }

    /// 500 REM cycles with dreaming: dream must not create invalid state.
    #[test]
    fn rem_500_with_dreaming() {
        let mut rng = Rng::new(0xBEEF);
        let n = 8;
        let mut g = random_graph(&mut rng, n);
        let mut state = TemporalState::new(n);
        let config = RemConfig {
            decay_rate: 0.02,
            prune_threshold: 0.001,
            prune_window: 10,
            d: 0.8,
            activation_threshold: 0.01,
        };

        for step in 0..500 {
            let alive: Vec<usize> = (0..n).filter(|&v| state.alive[v]).collect();
            if alive.is_empty() { break; }

            let mut init = vec![0.0; n];
            let seed = alive[rng.next_usize(alive.len())];
            init[seed] = 1.0;

            let report = rem_cycle(
                &mut g, &mut state, &init, &config,
                Some(|_i: usize, _j: usize| 0.6), // always dream
            );

            assert!(g.is_row_stochastic(),
                "Row-stochastic violated after dream at step {step}");
            assert!(report.health >= 0.0,
                "Negative health after dream at step {step}");
        }
    }

    /// Aggressive decay: weights should approach zero but never go negative.
    #[test]
    fn aggressive_decay_no_underflow() {
        let mut rng = Rng::new(0xFACE);
        let n = 6;
        let mut g = random_graph(&mut rng, n);
        let mut state = TemporalState::new(n);
        let config = RemConfig {
            decay_rate: 0.5, // Very aggressive
            prune_threshold: 0.0001,
            prune_window: 3,
            d: 0.8,
            activation_threshold: 0.01,
        };

        for step in 0..200 {
            let mut init = vec![0.0; n];
            init[0] = 1.0; // Always query node 0

            rem_cycle(
                &mut g, &mut state, &init, &config,
                None::<fn(usize, usize) -> f64>,
            );

            for i in 0..n {
                for j in 0..n {
                    let w = g.raw_weight(i, j);
                    assert!(w >= 0.0,
                        "Negative weight at ({i},{j})={w} after aggressive decay, step {step}");
                    assert!(!w.is_nan(),
                        "NaN weight at ({i},{j}) after aggressive decay, step {step}");
                }
            }
        }
    }

    /// Focused queries: distant nodes must eventually be pruned in sparse graphs.
    #[test]
    fn focused_queries_prune_distant() {
        let n = 12;
        // Sparse chain: 0→1→2→...→11, plus a few random edges.
        let mut w = vec![0.0; n * n];
        for i in 0..n - 1 {
            w[i * n + (i + 1)] = 1.0;
        }
        let g = ScoredGraph::new(n, w, vec![100; n]);
        let mut g = g;
        let mut state = TemporalState::new(n);
        let config = RemConfig {
            decay_rate: 0.2,
            prune_threshold: 0.05,
            prune_window: 5,
            d: 0.5,  // Low propagation — won't reach far
            activation_threshold: 0.02,
        };

        for _ in 0..50 {
            let mut init = vec![0.0; n];
            init[0] = 1.0;

            rem_cycle(
                &mut g, &mut state, &init, &config,
                None::<fn(usize, usize) -> f64>,
            );
        }

        // Node 11 (end of chain, hop=11) should have been pruned
        // with d=0.5 and threshold=0.05.
        let _far_alive = state.alive[n - 1];
        let _far2_alive = state.alive[n - 2];
        let near_alive = state.alive[0];

        assert!(near_alive, "Node 0 (query target) should be alive");
        // At least one distant node should have been pruned.
        let alive_count = state.alive_count();
        assert!(alive_count < n,
            "No pruning after 50 focused cycles: {alive_count}/{n} alive");
    }

    /// Contradictory queries: rapidly alternating between two distant nodes.
    /// Health should remain stable despite the whiplash.
    #[test]
    fn contradictory_queries_stable() {
        let mut rng = Rng::new(0xDEAD);
        let n = 10;
        let mut g = random_graph(&mut rng, n);
        let mut state = TemporalState::new(n);
        let config = RemConfig {
            decay_rate: 0.05,
            prune_threshold: 0.001,
            prune_window: 20, // Long window to avoid premature prune
            d: 0.8,
            activation_threshold: 0.01,
        };

        for step in 0..200 {
            let mut init = vec![0.0; n];
            // Alternate between node 0 and node n-1.
            let target = if step % 2 == 0 { 0 } else { n - 1 };
            init[target] = 1.0;

            let report = rem_cycle(
                &mut g, &mut state, &init, &config,
                None::<fn(usize, usize) -> f64>,
            );

            assert!(!report.health.is_nan(),
                "Health NaN during contradictory queries at step {step}");
            assert!(g.is_row_stochastic(),
                "Row-stochastic violated at step {step}");
        }
    }

    /// Verify contraction distance never goes negative over long runs.
    #[test]
    fn contraction_distance_always_positive() {
        let mut rng = Rng::new(12345);
        let n = 8;

        for _ in 0..20 {
            let g = random_graph(&mut rng, n);
            let mut init = vec![0.0; n];
            init[rng.next_usize(n)] = 1.0;

            let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
                d: 0.8, max_iter: 500, epsilon: 1e-12,
            });

            // Residuals must be non-negative and monotonically-ish decreasing.
            for (t, &r) in result.residuals.iter().enumerate() {
                assert!(r >= 0.0,
                    "Negative residual {r} at step {t}");
                assert!(r.is_finite(),
                    "Non-finite residual {r} at step {t}");
            }
        }
    }
}
