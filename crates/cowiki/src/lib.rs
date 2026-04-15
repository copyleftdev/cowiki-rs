//! # cowiki
//!
//! Co-Wiki retrieval pipeline: the composition layer.
//!
//! Composes the five primitive crates into the full Co-Wiki pipeline:
//!
//! ```text
//! query → ignite → spread → select → articles
//!                                      ↕
//!                               rem_cycle (maintenance)
//! ```
//!
//! This crate is intentionally thin — it's glue, not logic.
//! All proven properties live in the primitive crates.

pub use scored_graph::ScoredGraph;
pub use spread::{
    spread as activate, NoThreshold, SigmoidThreshold, HardThreshold,
    ThresholdFn, SpreadConfig, SpreadResult,
};
pub use budget_knap::{select, Item, Selection};
pub use temporal_graph::{
    rem_cycle, TemporalState, RemConfig, HealthReport,
};
pub use chunk_quality::{
    recall, precision, f1, chunk_coherence, density_variance,
};

/// Full retrieval pipeline: ignite → spread → select.
///
/// Given a query's initial activation vector, propagate activation through
/// the graph and select articles that maximize total activation within
/// the token budget.
pub fn retrieve(
    graph: &ScoredGraph,
    initial_activation: &[f64],
    budget: u64,
    spread_config: &SpreadConfig,
) -> (Selection, SpreadResult) {
    // 1. Spread activation.
    let spread_result = activate(
        graph,
        initial_activation,
        &SigmoidThreshold::default(),
        spread_config,
    );

    // 2. Build items from activation + costs.
    let items: Vec<Item> = spread_result.activation.iter()
        .enumerate()
        .map(|(i, &score)| Item {
            score,
            cost: graph.cost(i),
        })
        .collect();

    // 3. Select within budget.
    let selection = select(&items, budget);

    (selection, spread_result)
}

/// Run a maintenance cycle on the graph (REM agent).
pub fn maintain(
    graph: &mut ScoredGraph,
    state: &mut TemporalState,
    query_activation: &[f64],
    config: &RemConfig,
) -> HealthReport {
    rem_cycle(
        graph,
        state,
        query_activation,
        config,
        None::<fn(usize, usize) -> f64>,
    )
}

/// Run a maintenance cycle with dream (backlink discovery).
pub fn maintain_with_dream<F>(
    graph: &mut ScoredGraph,
    state: &mut TemporalState,
    query_activation: &[f64],
    config: &RemConfig,
    similarity: F,
) -> HealthReport
where
    F: Fn(usize, usize) -> f64,
{
    rem_cycle(graph, state, query_activation, config, Some(similarity))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiki_graph() -> ScoredGraph {
        // A small wiki: 5 articles with backlinks.
        //
        //   0 → 1 → 2
        //   ↓       ↑
        //   3 → 4 ──┘
        ScoredGraph::new(
            5,
            vec![
                0.0, 1.0, 0.0, 1.0, 0.0,  // 0 → 1, 3
                0.0, 0.0, 1.0, 0.0, 0.0,  // 1 → 2
                0.0, 0.0, 0.0, 0.0, 0.0,  // 2 (leaf)
                0.0, 0.0, 0.0, 0.0, 1.0,  // 3 → 4
                0.0, 0.0, 1.0, 0.0, 0.0,  // 4 → 2
            ],
            vec![100, 200, 50, 150, 80],
        )
    }

    #[test]
    fn end_to_end_retrieval() {
        let g = wiki_graph();
        let mut init = vec![0.0; 5];
        init[0] = 1.0; // Query activates article 0.

        let (selection, spread_result) = retrieve(
            &g,
            &init,
            500, // Budget: can fit ~3-4 articles.
            &SpreadConfig::default(),
        );

        // Should have selected some articles.
        assert!(!selection.indices.is_empty());
        // Budget should be respected.
        assert!(selection.total_cost <= 500);
        // Article 0 should be in the selection (directly activated).
        assert!(selection.indices.contains(&0));
        // Activation should have converged.
        assert!(spread_result.converged);
    }

    #[test]
    fn end_to_end_maintenance() {
        let mut g = wiki_graph();
        let mut state = TemporalState::new(5);

        // Run several maintenance cycles focusing on article 0.
        for _ in 0..10 {
            let mut init = vec![0.0; 5];
            init[0] = 1.0;

            let report = maintain(
                &mut g, &mut state, &init,
                &RemConfig {
                    decay_rate: 0.1,
                    prune_threshold: 0.05,
                    prune_window: 3,
                    d: 0.7,
                    activation_threshold: 0.02,
                },
            );

            assert!(report.health >= 0.0);
        }
    }

    #[test]
    fn graph_beats_topk() {
        // Article 2 is 2 hops from query (0 → 1 → 2) but very cheap.
        // Top-k by activation would miss it; density-based retrieval catches it.
        let g = wiki_graph();
        let mut init = vec![0.0; 5];
        init[0] = 1.0;

        let (_selection, spread) = retrieve(&g, &init, 200, &SpreadConfig {
            d: 0.85,
            ..Default::default()
        });

        // Article 2 (cost=50) should be reachable via spreading activation.
        let a2_activation = spread.activation[2];
        assert!(a2_activation > 0.0,
            "Multi-hop article 2 should have non-zero activation");
    }
}
