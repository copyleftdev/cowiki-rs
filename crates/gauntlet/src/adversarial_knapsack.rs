//! Adversarial constructions for the knapsack ½-bound.
//!
//! These are worst-case inputs from the approximation algorithm literature,
//! designed to push the greedy as close to ½ as possible without breaking it.

#[cfg(test)]
mod tests {
    use budget_knap::{select, optimal_bruteforce, Item};

    /// Classic worst case for pure greedy-by-density (without the single-item fix):
    /// One large valuable item vs many tiny items that collectively have less value.
    /// The modified greedy should handle this via max(greedy, best_single).
    #[test]
    fn classic_worst_case() {
        // Item A: score=1.0, cost=budget (fills entirely).
        // Items B_i: score=ε, cost=1. There are `budget` of them, total score = budget*ε.
        // For ε small enough, pure density-greedy picks all B_i (density=ε > 1.0/budget).
        // Modified greedy takes max(sum(B_i), A) = max(budget*ε, 1.0) = 1.0.
        let budget = 100u64;
        let eps = 0.001;
        let mut items = vec![Item { score: 1.0, cost: budget }]; // Item A
        for _ in 0..budget {
            items.push(Item { score: eps, cost: 1 }); // Items B_i
        }

        let sel = select(&items, budget);
        let opt = optimal_bruteforce(&items[..items.len().min(20)], budget);

        assert!(sel.total_score >= 0.5 * opt.total_score - 1e-9,
            "Modified greedy={}, opt={}", sel.total_score, opt.total_score);

        // Verify the fix works: modified greedy should pick item A (score=1.0).
        assert!(sel.total_score >= 1.0 - 1e-9,
            "Should pick the large item: got {}", sel.total_score);
    }

    /// Many items with identical density. Tie-breaking shouldn't cause issues.
    #[test]
    fn identical_density_tiebreak() {
        // All items have score/cost = 0.01.
        let items: Vec<Item> = (0..20).map(|i| Item {
            score: f64::from(i + 1) * 0.01,
            cost: (i + 1) as u64,
        }).collect();

        let sel = select(&items, 50);
        assert!(sel.total_cost <= 50, "Budget violated");
        // Should still achieve a reasonable fraction of optimal.
        let opt = optimal_bruteforce(&items, 50);
        if opt.total_score > 0.0 {
            assert!(sel.total_score >= 0.5 * opt.total_score - 1e-9);
        }
    }

    /// All items exceed budget. Selection must be empty.
    #[test]
    fn all_exceed_budget() {
        let items: Vec<Item> = (0..10).map(|_| Item { score: 1.0, cost: 1000 }).collect();
        let sel = select(&items, 100);
        assert!(sel.indices.is_empty());
        assert_eq!(sel.total_score, 0.0);
        assert_eq!(sel.total_cost, 0);
    }

    /// Budget = 0. Nothing can be selected.
    #[test]
    fn zero_budget() {
        let items = vec![Item { score: 1.0, cost: 1 }];
        let sel = select(&items, 0);
        assert!(sel.indices.is_empty());
    }

    /// Single item that exactly fills the budget.
    #[test]
    fn exact_fill() {
        let items = vec![
            Item { score: 0.5, cost: 100 },
            Item { score: 0.3, cost: 100 },
        ];
        let sel = select(&items, 100);
        assert_eq!(sel.total_cost, 100);
        assert!(sel.total_score >= 0.5 - 1e-9, "Should pick best item");
    }

    /// Adversarial density inversion: high-density items are low-value,
    /// low-density items are high-value. Tests that modified greedy
    /// doesn't get fooled.
    #[test]
    fn density_inversion() {
        let items = vec![
            // High density, low value.
            Item { score: 0.01, cost: 1 },
            Item { score: 0.01, cost: 1 },
            Item { score: 0.01, cost: 1 },
            Item { score: 0.01, cost: 1 },
            Item { score: 0.01, cost: 1 },
            // Low density, high value.
            Item { score: 5.0, cost: 500 },
        ];
        let budget = 500;

        let sel = select(&items, budget);
        let opt = optimal_bruteforce(&items, budget);

        assert!(sel.total_score >= 0.5 * opt.total_score - 1e-9,
            "greedy={}, opt={}", sel.total_score, opt.total_score);
    }

    /// Stress test: ½-bound over many random instances.
    #[test]
    fn half_bound_stress() {
        let mut rng = 0x12345u64;

        for trial in 0..500 {
            // XorShift for reproducibility.
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;

            let n = 3 + (rng as usize % 13);
            let budget = 50 + (rng % 500);

            let items: Vec<Item> = (0..n).map(|_| {
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                let score = (rng % 1000) as f64 / 1000.0;
                rng ^= rng << 13;
                rng ^= rng >> 7;
                rng ^= rng << 17;
                let cost = 1 + rng % 300;
                Item { score, cost }
            }).collect();

            let sel = select(&items, budget);
            let opt = optimal_bruteforce(&items, budget);

            assert!(sel.total_cost <= budget,
                "Trial {trial}: budget violated");

            if opt.total_score > 0.0 {
                let ratio = sel.total_score / opt.total_score;
                assert!(ratio >= 0.5 - 1e-9,
                    "Trial {trial}: ratio={ratio:.4} (greedy={}, opt={})",
                    sel.total_score, opt.total_score);
            }
        }
    }

    /// Pathological: items with score but zero cost should be handled.
    /// (Cost must be > 0 per Item definition, but let's test cost=1.)
    #[test]
    fn near_free_items() {
        let items: Vec<Item> = (0..100).map(|i| Item {
            score: (f64::from(i) + 1.0) * 0.01,
            cost: 1,
        }).collect();

        let sel = select(&items, 10);
        assert_eq!(sel.total_cost, 10);
        // Should pick the 10 highest-score items.
        assert!(sel.indices.len() == 10);
    }
}
