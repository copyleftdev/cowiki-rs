//! Floating-point torture tests.
//!
//! Target: denormals, near-zero, near-overflow, precision loss,
//! cancellation, and every other way IEEE 754 can betray you.

#[cfg(test)]
mod tests {
    use scored_graph::ScoredGraph;
    use spread::{spread, NoThreshold, HardThreshold, SigmoidThreshold, SpreadConfig};
    use budget_knap::{select, Item};

    /// Graph with machine-epsilon edge weights.
    fn epsilon_graph(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    w[i * n + j] = f64::MIN_POSITIVE;
                }
            }
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Graph where one edge is enormous and all others are tiny.
    fn skewed_graph(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        // One huge edge.
        if n >= 2 {
            w[1] = 1e15;
        }
        // Tiny edges everywhere else.
        for i in 0..n {
            for j in 0..n {
                if i != j && w[i * n + j] == 0.0 {
                    w[i * n + j] = 1e-15;
                }
            }
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Graph with weights that sum to near-overflow before normalization.
    ///
    /// Sized to the storage type's limit (`f32`): `f32::MAX / (n·n)` still
    /// exercises the "huge row sum, tiny quotient" path without producing
    /// infinities when the f64 inputs are demoted at construction.
    fn near_overflow_graph(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        let huge = f32::MAX as f64 / (n as f64 * n as f64);
        for i in 0..n {
            for j in 0..n {
                if i != j {
                    w[i * n + j] = huge;
                }
            }
        }
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Graph with alternating zero and non-zero rows (some nodes have no out-edges).
    fn sparse_rows(n: usize) -> ScoredGraph {
        let mut w = vec![0.0; n * n];
        for i in (0..n).step_by(2) {
            for j in 0..n {
                if i != j {
                    w[i * n + j] = 1.0;
                }
            }
        }
        // Odd rows have NO outgoing edges.
        ScoredGraph::new(n, w, vec![100; n])
    }

    /// Machine-epsilon weights: normalization should still produce valid stochastic matrix.
    #[test]
    fn epsilon_weights_valid() {
        let g = epsilon_graph(5);
        assert!(g.is_row_stochastic(),
            "Epsilon weights broke row-stochastic invariant");
    }

    /// Machine-epsilon: spreading activation must not produce NaN.
    #[test]
    fn epsilon_no_nan() {
        let g = epsilon_graph(5);
        let init = vec![1.0, 0.0, 0.0, 0.0, 0.0];

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

        for (i, &a) in result.activation.iter().enumerate() {
            assert!(!a.is_nan(), "NaN at node {i} with epsilon weights");
            assert!(!a.is_infinite(), "Inf at node {i} with epsilon weights");
        }
    }

    /// Skewed graph: one 1e15 edge vs 1e-15 edges.
    /// After normalization, the huge edge should dominate its row.
    #[test]
    fn skewed_normalization_stable() {
        let g = skewed_graph(5);
        assert!(g.is_row_stochastic());

        // Node 0's out-edges: one huge, rest tiny.
        // After normalization, adj(0,1) should be ≈ 1.0.
        assert!(g.adj(0, 1) > 0.99,
            "Huge edge should dominate: adj(0,1) = {}", g.adj(0, 1));
    }

    /// Skewed graph: spread must not NaN even with extreme weight disparity.
    #[test]
    fn skewed_spread_stable() {
        let g = skewed_graph(5);
        let init = vec![1.0, 0.0, 0.0, 0.0, 0.0];

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig {
            d: 0.9, max_iter: 200, epsilon: 1e-10,
        });

        for (i, &a) in result.activation.iter().enumerate() {
            assert!(!a.is_nan(), "NaN at node {i} with skewed weights");
            assert!(a >= -1e-10, "Negative activation {a} at node {i}");
        }
    }

    /// Near-overflow weights: normalization should tame them.
    #[test]
    fn near_overflow_normalization() {
        let g = near_overflow_graph(5);
        assert!(g.is_row_stochastic(),
            "Near-overflow weights broke normalization");

        // After normalization, values should be ~1/(n-1).
        let expected = 1.0 / 4.0;
        assert!((g.adj(0, 1) - expected).abs() < 1e-9,
            "Expected adj ≈ {expected}, got {}", g.adj(0, 1));
    }

    /// Near-overflow: spread must converge without blowup.
    #[test]
    fn near_overflow_spread() {
        let g = near_overflow_graph(5);
        let init = vec![1.0, 0.0, 0.0, 0.0, 0.0];

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

        assert!(result.converged, "Did not converge with near-overflow weights");
        for &a in &result.activation {
            assert!(a.is_finite(), "Non-finite activation with near-overflow");
        }
    }

    /// Sparse rows: nodes with zero out-degree should not cause division by zero.
    #[test]
    fn sparse_rows_no_div_zero() {
        let g = sparse_rows(6);
        assert!(g.is_row_stochastic());

        // Odd nodes (no out-edges): their adj row should be all zeros.
        for j in 0..6 {
            assert_eq!(g.adj(1, j), 0.0,
                "Node 1 has no edges but adj(1,{j}) = {}", g.adj(1, j));
        }
    }

    /// Activation at EXACTLY the hard threshold boundary.
    /// This is the known weakness from Finding 1.
    #[test]
    fn exact_threshold_boundary() {
        let g = ScoredGraph::new(
            3,
            vec![
                0.0, 1.0, 0.0,
                0.0, 0.0, 1.0,
                1.0, 0.0, 0.0,
            ],
            vec![100; 3],
        );

        // Set initial activation to EXACTLY the threshold value.
        let theta = 0.01;
        let init = vec![theta, 0.0, 0.0];

        let result = spread(&g, &init, &HardThreshold(theta), &SpreadConfig {
            d: 0.8, max_iter: 500, epsilon: 1e-10,
        });

        // We don't assert convergence (hard threshold can limit-cycle),
        // but we MUST assert no NaN/Inf/negative.
        for (i, &a) in result.activation.iter().enumerate() {
            assert!(!a.is_nan(), "NaN at boundary: node {i}");
            assert!(!a.is_infinite(), "Inf at boundary: node {i}");
            assert!(a >= -1e-10, "Negative at boundary: node {i} = {a}");
        }
    }

    /// All-zero initial activation should produce all-zero result.
    #[test]
    fn zero_initial_stays_zero() {
        let g = ScoredGraph::new(
            3,
            vec![0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0],
            vec![100; 3],
        );
        let init = vec![0.0; 3];

        let result = spread(&g, &init, &NoThreshold, &SpreadConfig::default());

        for (i, &a) in result.activation.iter().enumerate() {
            assert_eq!(a, 0.0, "Non-zero activation from zero input at node {i}: {a}");
        }
    }

    /// Knapsack with zero-score items: should select nothing.
    #[test]
    fn knapsack_zero_scores() {
        let items = vec![
            Item { score: 0.0, cost: 100 },
            Item { score: 0.0, cost: 200 },
        ];
        let sel = select(&items, 1000);
        assert_eq!(sel.total_score, 0.0);
        assert!(sel.indices.is_empty());
    }

    /// Knapsack with item cost = budget exactly.
    #[test]
    fn knapsack_exact_fit() {
        let items = vec![
            Item { score: 1.0, cost: 500 },
            Item { score: 0.5, cost: 500 },
        ];
        let sel = select(&items, 500);
        assert_eq!(sel.total_cost, 500);
        assert!(sel.total_score >= 1.0 - 1e-9, "Should pick the best single item");
    }

    /// Knapsack with extreme density disparity.
    #[test]
    fn knapsack_density_extremes() {
        let items = vec![
            Item { score: 0.001, cost: 1 },    // density = 0.001
            Item { score: 100.0, cost: 10000 }, // density = 0.01 but huge
        ];
        // Budget only fits the tiny item.
        let sel = select(&items, 5);
        assert!(sel.total_cost <= 5);
    }

    /// Sigmoid threshold should never produce NaN, even with extreme inputs.
    #[test]
    fn sigmoid_extreme_inputs() {
        use spread::ThresholdFn;

        let s = SigmoidThreshold::new(0.5, 100.0); // very steep

        // Extreme values that could cause exp() overflow.
        for &input in &[0.0, 1.0, -1.0, 1e10, -1e10, f64::MIN_POSITIVE, 0.5] {
            let result = s.apply(input);
            assert!(result.is_finite(),
                "Sigmoid NaN/Inf for input {input}: got {result}");
        }
    }
}
