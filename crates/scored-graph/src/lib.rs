//! # scored-graph
//!
//! Weighted directed graph where nodes carry a **cost** and edges carry a **weight**.
//!
//! The adjacency matrix is row-stochastic (each row sums to 1.0), which is an
//! invariant enforced at construction. This normalization is required for the
//! spreading activation contraction proof.
//!
//! ## Formal definition
//!
//! ```text
//! G = (V, E, w, τ, κ)
//!
//! V = {v₁, …, vₙ}         nodes
//! E ⊆ V × V                directed edges
//! w: E → ℝ⁺                edge weight (association strength)
//! τ: V → ℕ                 node cost (tokens, bytes, etc.)
//! κ: V → 2^C               category assignment
//! ```

use std::collections::VecDeque;

/// A weighted directed graph with row-stochastic adjacency and per-node costs.
#[derive(Debug, Clone)]
pub struct ScoredGraph {
    /// Number of nodes.
    n: usize,
    /// Raw (unnormalized) weight matrix, row-major: `raw[i * n + j]` = weight of edge i → j.
    raw: Vec<f64>,
    /// Row-stochastic adjacency: `adj[i * n + j]` = `raw[i,j] / Σ_k raw[i,k]`.
    adj: Vec<f64>,
    /// Token cost per node. Must be > 0 for all nodes.
    costs: Vec<u64>,
    /// Category bitset per node (simple: just a `Vec<u64>` used as a bitfield).
    categories: Vec<u64>,
}

impl ScoredGraph {
    /// Build a graph from a raw weight matrix and node costs.
    ///
    /// # Panics
    /// - If `weights.len() != n * n`
    /// - If `costs.len() != n`
    /// - If any weight is negative
    /// - If any cost is zero
    pub fn new(n: usize, weights: Vec<f64>, costs: Vec<u64>) -> Self {
        assert_eq!(weights.len(), n * n, "weights must be n×n");
        assert_eq!(costs.len(), n, "costs must have n elements");
        assert!(weights.iter().all(|&w| w >= 0.0), "weights must be non-negative");
        assert!(costs.iter().all(|&c| c > 0), "costs must be positive");

        // Zero out diagonal (no self-loops).
        let mut raw = weights;
        for i in 0..n {
            raw[i * n + i] = 0.0;
        }

        let adj = row_stochastize(&raw, n);

        Self {
            n,
            raw,
            adj,
            costs,
            categories: vec![0; n],
        }
    }

    /// Build with category assignments.
    #[must_use]
    pub fn with_categories(mut self, categories: Vec<u64>) -> Self {
        assert_eq!(categories.len(), self.n);
        self.categories = categories;
        self
    }

    /// Number of nodes.
    #[inline]
    pub fn len(&self) -> usize {
        self.n
    }

    /// Whether the graph has no nodes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Row-stochastic adjacency value W[i][j].
    #[inline]
    pub fn adj(&self, i: usize, j: usize) -> f64 {
        self.adj[i * self.n + j]
    }

    /// Raw (unnormalized) weight of edge i → j.
    #[inline]
    pub fn raw_weight(&self, i: usize, j: usize) -> f64 {
        self.raw[i * self.n + j]
    }

    /// Token cost of node v.
    #[inline]
    pub fn cost(&self, v: usize) -> u64 {
        self.costs[v]
    }

    /// Slice of all costs.
    #[inline]
    pub fn costs(&self) -> &[u64] {
        &self.costs
    }

    /// Full row-stochastic adjacency matrix as a flat slice (row-major).
    #[inline]
    pub fn adj_matrix(&self) -> &[f64] {
        &self.adj
    }

    /// Full raw weight matrix as a flat slice (row-major).
    #[inline]
    pub fn raw_matrix(&self) -> &[f64] {
        &self.raw
    }

    /// Mutable access to the raw weight matrix. Caller must call
    /// [`renormalize`](Self::renormalize) after modification.
    #[inline]
    pub fn raw_matrix_mut(&mut self) -> &mut [f64] {
        &mut self.raw
    }

    /// Recompute the row-stochastic adjacency from current raw weights.
    pub fn renormalize(&mut self) {
        self.adj = row_stochastize(&self.raw, self.n);
    }

    /// Outgoing neighbors of node `v` (nodes that `v` links to).
    pub fn neighbors_out(&self, v: usize) -> Vec<usize> {
        (0..self.n).filter(|&j| self.raw[v * self.n + j] > 0.0).collect()
    }

    /// Incoming neighbors of node `v` (nodes that link to `v`).
    pub fn neighbors_in(&self, v: usize) -> Vec<usize> {
        (0..self.n).filter(|&i| self.raw[i * self.n + v] > 0.0).collect()
    }

    /// BFS shortest path length from `src` to `dst`. Returns `None` if unreachable.
    pub fn shortest_path(&self, src: usize, dst: usize) -> Option<usize> {
        if src == dst {
            return Some(0);
        }
        let mut visited = vec![false; self.n];
        visited[src] = true;
        let mut queue = VecDeque::new();
        queue.push_back((src, 0usize));

        while let Some((node, depth)) = queue.pop_front() {
            for nb in self.neighbors_out(node) {
                if nb == dst {
                    return Some(depth + 1);
                }
                if !visited[nb] {
                    visited[nb] = true;
                    queue.push_back((nb, depth + 1));
                }
            }
        }
        None
    }

    /// Verify the row-stochastic invariant: every row with outgoing edges sums to 1.0.
    pub fn is_row_stochastic(&self) -> bool {
        for i in 0..self.n {
            let sum: f64 = (0..self.n).map(|j| self.adj[i * self.n + j]).sum();
            let has_edges = (0..self.n).any(|j| self.raw[i * self.n + j] > 0.0);
            if has_edges && (sum - 1.0).abs() > 1e-9 {
                return false;
            }
        }
        true
    }
}

/// Row-stochastic normalization: each row sums to 1.
/// Rows with zero out-degree remain all zeros.
fn row_stochastize(raw: &[f64], n: usize) -> Vec<f64> {
    let mut adj = raw.to_vec();
    for i in 0..n {
        let row_start = i * n;
        let row_sum: f64 = adj[row_start..row_start + n].iter().sum();
        if row_sum > 0.0 {
            for j in 0..n {
                adj[row_start + j] /= row_sum;
            }
        }
    }
    adj
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_construction() {
        let g = ScoredGraph::new(
            3,
            vec![
                0.0, 1.0, 2.0,
                0.5, 0.0, 0.5,
                0.0, 0.0, 0.0,
            ],
            vec![100, 200, 150],
        );
        assert_eq!(g.len(), 3);
        assert!(g.is_row_stochastic());
        assert!((g.adj(0, 1) - 1.0 / 3.0).abs() < 1e-9);
        assert!((g.adj(0, 2) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(g.cost(1), 200);
    }

    #[test]
    fn self_loops_zeroed() {
        let g = ScoredGraph::new(
            2,
            vec![5.0, 1.0, 1.0, 5.0],
            vec![100, 100],
        );
        assert_eq!(g.raw_weight(0, 0), 0.0);
        assert_eq!(g.raw_weight(1, 1), 0.0);
    }

    #[test]
    fn shortest_path_works() {
        let g = ScoredGraph::new(
            4,
            vec![
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
                0.0, 0.0, 0.0, 0.0,
            ],
            vec![100; 4],
        );
        assert_eq!(g.shortest_path(0, 3), Some(3));
        assert_eq!(g.shortest_path(3, 0), None);
        assert_eq!(g.shortest_path(0, 0), Some(0));
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

    proptest! {
        /// Row-stochastic invariant holds for any random graph.
        #[test]
        fn row_stochastic_invariant(g in arb_graph(15)) {
            prop_assert!(g.is_row_stochastic(),
                "Row-stochastic invariant violated for n={}", g.len());
        }

        /// No self-loops in any constructed graph.
        #[test]
        fn no_self_loops(g in arb_graph(15)) {
            for i in 0..g.len() {
                prop_assert_eq!(g.raw_weight(i, i), 0.0,
                    "Self-loop at node {}", i);
            }
        }

        /// All raw weights are non-negative.
        #[test]
        fn non_negative_weights(g in arb_graph(15)) {
            for i in 0..g.len() {
                for j in 0..g.len() {
                    prop_assert!(g.raw_weight(i, j) >= 0.0);
                }
            }
        }

        /// Shortest path to self is always 0.
        #[test]
        fn self_distance_zero(g in arb_graph(10)) {
            for v in 0..g.len() {
                prop_assert_eq!(g.shortest_path(v, v), Some(0));
            }
        }
    }
}
