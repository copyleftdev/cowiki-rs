//! # budget-knap
//!
//! Budget-constrained selection with a provable ≥ ½ OPT guarantee.
//!
//! Given items with scores and costs, select a subset that maximizes total
//! score without exceeding a budget. This is the 0-1 knapsack problem.
//!
//! The modified greedy algorithm:
//! ```text
//! result = max(greedy_by_density, best_single_item)
//! ```
//! guarantees `result ≥ ½ · OPT`.
//!
//! ## Why this exists as a separate crate
//!
//! Any context-window-stuffing problem is a knapsack: token budgets, memory
//! budgets, time budgets, API rate limits. This crate is domain-agnostic.

/// An item with a score to maximize and a cost to budget.
#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub score: f64,
    pub cost: u64,
}

/// Result of a budget-constrained selection.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Indices of selected items.
    pub indices: Vec<usize>,
    /// Total score of selected items.
    pub total_score: f64,
    /// Total cost of selected items.
    pub total_cost: u64,
}

/// Modified greedy knapsack selection. **Guarantees `total_score ≥ ½ · OPT`.**
///
/// Two strategies are tried and the better result is returned:
/// 1. Greedy by density: sort by `score/cost` descending, fill greedily.
/// 2. Best single item that fits within budget.
///
/// ## Proven properties (P3.1–P3.4)
///
/// - **P3.1**: `select().total_score ≥ 0.5 * optimal().total_score`
/// - **P3.2**: `select().total_cost ≤ budget`
/// - **P5.1**: When all costs are equal, this degenerates to top-k
/// - **P5.2**: When costs vary, density-based selection can strictly outperform top-k
pub fn select(items: &[Item], budget: u64) -> Selection {
    // Strategy 1: greedy by density.
    let density_sel = greedy_by_density(items, budget);

    // Strategy 2: best single item that fits.
    let single_sel = best_single(items, budget);

    if single_sel.total_score > density_sel.total_score {
        single_sel
    } else {
        density_sel
    }
}

/// Pure greedy by activation density ρ = score / cost.
pub fn greedy_by_density(items: &[Item], budget: u64) -> Selection {
    let mut order: Vec<usize> = (0..items.len())
        .filter(|&i| items[i].score > 0.0 && items[i].cost > 0)
        .collect();
    order.sort_by(|&a, &b| {
        let da = items[a].score / items[a].cost as f64;
        let db = items[b].score / items[b].cost as f64;
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut indices = Vec::new();
    let mut total_score = 0.0;
    let mut total_cost = 0u64;

    for idx in order {
        let item = &items[idx];
        if total_cost + item.cost <= budget {
            indices.push(idx);
            total_score += item.score;
            total_cost += item.cost;
        }
    }

    Selection { indices, total_score, total_cost }
}

/// Best single item that fits within budget.
fn best_single(items: &[Item], budget: u64) -> Selection {
    let mut best_idx = None;
    let mut best_score = 0.0;

    for (i, item) in items.iter().enumerate() {
        if item.cost <= budget && item.score > best_score {
            best_score = item.score;
            best_idx = Some(i);
        }
    }

    match best_idx {
        Some(idx) => Selection {
            total_cost: items[idx].cost,
            total_score: best_score,
            indices: vec![idx],
        },
        None => Selection {
            indices: vec![],
            total_score: 0.0,
            total_cost: 0,
        },
    }
}

/// Brute-force optimal selection. Only feasible for `items.len() ≤ 20`.
/// Used in property tests to verify the greedy bound.
pub fn optimal_bruteforce(items: &[Item], budget: u64) -> Selection {
    let n = items.len();
    assert!(n <= 20, "brute force only feasible for n <= 20, got {n}");

    let mut best = Selection { indices: vec![], total_score: 0.0, total_cost: 0 };

    for mask in 0..(1u32 << n) {
        let mut total_score = 0.0;
        let mut total_cost = 0u64;
        let mut indices = Vec::new();

        for (i, item) in items.iter().enumerate() {
            if mask & (1 << i) != 0 {
                total_cost += item.cost;
                if total_cost > budget {
                    break;
                }
                total_score += item.score;
                indices.push(i);
            }
        }

        if total_cost <= budget && total_score > best.total_score {
            best = Selection { indices, total_score, total_cost };
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_hot_beats_large_warm() {
        let items = vec![
            Item { score: 0.9, cost: 50 },
            Item { score: 0.5, cost: 400 },
        ];
        let sel = select(&items, 100);
        assert!(sel.indices.contains(&0));
        assert!(!sel.indices.contains(&1));
    }

    #[test]
    fn empty_items() {
        let sel = select(&[], 1000);
        assert!(sel.indices.is_empty());
        assert_eq!(sel.total_score, 0.0);
    }

    #[test]
    fn nothing_fits() {
        let items = vec![Item { score: 1.0, cost: 500 }];
        let sel = select(&items, 100);
        assert!(sel.indices.is_empty());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_items(max_n: usize) -> impl Strategy<Value = Vec<Item>> {
        proptest::collection::vec(
            (0.0..1.0f64, 1..300u64).prop_map(|(s, c)| Item { score: s, cost: c }),
            1..=max_n,
        )
    }

    proptest! {
        /// P3.1: Modified greedy achieves >= 1/2 of optimal.
        #[test]
        fn half_optimality(items in arb_items(15), budget in 50..1000u64) {
            let greedy = select(&items, budget);
            let optimal = optimal_bruteforce(&items, budget);

            if optimal.total_score > 0.0 {
                let ratio = greedy.total_score / optimal.total_score;
                prop_assert!(ratio >= 0.5 - 1e-9,
                    "Greedy ratio {} < 0.5 (greedy={}, opt={})",
                    ratio, greedy.total_score, optimal.total_score);
            }
        }

        /// P3.2: Budget is never exceeded.
        #[test]
        fn budget_respected(items in arb_items(20), budget in 50..2000u64) {
            let sel = select(&items, budget);
            prop_assert!(sel.total_cost <= budget,
                "Budget violated: used {}, budget={}", sel.total_cost, budget);
        }

        /// P5.1: When all costs are equal, greedy_by_density == top-k by score.
        #[test]
        fn fixed_cost_is_topk(
            scores in proptest::collection::vec(0.0..1.0f64, 3..15),
            cost in 50..200u64,
            budget in 100..1000u64,
        ) {
            let items: Vec<Item> = scores.iter()
                .map(|&s| Item { score: s, cost })
                .collect();

            let density_sel = greedy_by_density(&items, budget);

            // Top-k by score.
            let mut by_score: Vec<usize> = (0..items.len())
                .filter(|&i| items[i].score > 0.0)
                .collect();
            by_score.sort_by(|&a, &b|
                items[b].score.partial_cmp(&items[a].score)
                    .unwrap_or(std::cmp::Ordering::Equal));

            let k = (budget / cost) as usize;
            let topk: Vec<usize> = by_score.into_iter().take(k).collect();

            let density_set: std::collections::HashSet<_> = density_sel.indices.iter().copied().collect();
            let topk_set: std::collections::HashSet<_> = topk.iter().copied().collect();

            prop_assert_eq!(density_set, topk_set,
                "With fixed costs, density and top-k should select same items");
        }
    }
}
