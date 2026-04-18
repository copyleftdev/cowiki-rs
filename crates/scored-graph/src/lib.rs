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
///
/// Keeps both a dense adjacency (for random access, persistence, neighborhood
/// queries) and a CSR-encoded transpose (`Wᵀ`) for the spreading activation
/// hot loop — the operator is `T(a) = (1-d)·a⁰ + d·Wᵀ·f(a)`, so iterating by
/// columns of W (== rows of Wᵀ) against sparse edges is the cache-friendly,
/// arithmetic-minimal form.
/// Edge-weight storage type. `f32` halves the dense-matrix footprint vs `f64`
/// (so n² storage tops out at `4 · n²` bytes, not `8 · n²`). Spread accumulates
/// SpMV contributions in `f64` to keep rounding error well below the Lipschitz
/// bound of the operator; row-stochastic tolerance in [`Self::is_row_stochastic`]
/// is relaxed to match the ~7-digit f32 mantissa.
pub type EdgeWeight = f32;

#[derive(Debug, Clone)]
pub struct ScoredGraph {
    /// Number of nodes.
    n: usize,
    /// Raw (unnormalized) weight matrix, row-major: `raw[i * n + j]` = weight of edge i → j.
    raw: Vec<EdgeWeight>,
    /// Row-stochastic adjacency: `adj[i * n + j]` = `raw[i,j] / Σ_k raw[i,k]`.
    adj: Vec<EdgeWeight>,
    /// CSR transpose of `adj`: rows are columns of W. `adj_t_row_ptr[j+1] - adj_t_row_ptr[j]`
    /// is the in-degree of node j.
    adj_t_row_ptr: Vec<usize>,
    adj_t_col_idx: Vec<usize>,
    adj_t_values:  Vec<EdgeWeight>,
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

        // Demote to f32 and zero the diagonal.
        let mut raw: Vec<EdgeWeight> = weights.iter().map(|&w| w as EdgeWeight).collect();
        for i in 0..n {
            raw[i * n + i] = 0.0;
        }

        let adj = row_stochastize(&raw, n);
        let (adj_t_row_ptr, adj_t_col_idx, adj_t_values) = build_csr_transpose(&adj, n);

        Self {
            n,
            raw,
            adj,
            adj_t_row_ptr,
            adj_t_col_idx,
            adj_t_values,
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

    /// Row-stochastic adjacency value W[i][j]. Returned as `f64` for caller
    /// ergonomics; storage is `f32`, so the cast is free.
    #[inline]
    pub fn adj(&self, i: usize, j: usize) -> f64 {
        self.adj[i * self.n + j] as f64
    }

    /// Raw (unnormalized) weight of edge i → j. See [`Self::adj`] for the f32/f64 note.
    #[inline]
    pub fn raw_weight(&self, i: usize, j: usize) -> f64 {
        self.raw[i * self.n + j] as f64
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
    /// Storage is `f32` — accumulate in `f64` when summing many entries.
    #[inline]
    pub fn adj_matrix(&self) -> &[EdgeWeight] {
        &self.adj
    }

    /// CSR-encoded transpose of the row-stochastic adjacency. Tuple is
    /// `(row_ptr, col_idx, values)` where for column `j` of W (row `j` of Wᵀ):
    /// `col_idx[row_ptr[j]..row_ptr[j+1]]` are source node indices and
    /// `values[...]` are the edge weights (`f32`; accumulate in `f64`).
    /// Sized by edge count, not n².
    #[inline]
    pub fn adj_transpose_csr(&self) -> (&[usize], &[usize], &[EdgeWeight]) {
        (&self.adj_t_row_ptr, &self.adj_t_col_idx, &self.adj_t_values)
    }

    /// Full raw weight matrix as a flat slice (row-major).
    #[inline]
    pub fn raw_matrix(&self) -> &[EdgeWeight] {
        &self.raw
    }

    /// Mutable access to the raw weight matrix. Caller must call
    /// [`renormalize`](Self::renormalize) after modification. Weights are `f32`.
    #[inline]
    pub fn raw_matrix_mut(&mut self) -> &mut [EdgeWeight] {
        &mut self.raw
    }

    /// Recompute the row-stochastic adjacency from current raw weights and
    /// rebuild the CSR transpose sidecar.
    pub fn renormalize(&mut self) {
        self.adj = row_stochastize(&self.raw, self.n);
        let (rp, ci, v) = build_csr_transpose(&self.adj, self.n);
        self.adj_t_row_ptr = rp;
        self.adj_t_col_idx = ci;
        self.adj_t_values  = v;
    }

    /// Outgoing neighbors of node `v` (nodes that `v` links to).
    pub fn neighbors_out(&self, v: usize) -> Vec<usize> {
        (0..self.n).filter(|&j| self.raw[v * self.n + j] > 0.0).collect()
    }

    /// Incoming neighbors of node `v` (nodes that link to `v`).
    pub fn neighbors_in(&self, v: usize) -> Vec<usize> {
        (0..self.n).filter(|&i| self.raw[i * self.n + v] > 0.0).collect()
    }

    /// Iterate (i, j, weight) triples for every positive raw edge, accumulating
    /// the row sum in f64 to preserve precision across many entries.
    #[allow(dead_code)]
    fn row_sum_f64(&self, i: usize) -> f64 {
        let row = &self.adj[i * self.n..(i + 1) * self.n];
        row.iter().map(|&w| w as f64).sum()
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
    ///
    /// Tolerance is scaled for f32 storage: the row sum is accumulated in f64,
    /// but the per-edge rounding floor is ~1e-7 — with up to n nonzeros per row,
    /// the worst-case residual grows linearly. 1e-5 comfortably covers
    /// n ≤ 100 000 while still catching any structural violation.
    pub fn is_row_stochastic(&self) -> bool {
        for i in 0..self.n {
            let sum: f64 = (0..self.n).map(|j| self.adj[i * self.n + j] as f64).sum();
            let has_edges = (0..self.n).any(|j| self.raw[i * self.n + j] > 0.0);
            if has_edges && (sum - 1.0).abs() > 1e-5 {
                return false;
            }
        }
        true
    }
}

/// Row-stochastic normalization: each row sums to 1.
/// Rows with zero out-degree remain all zeros. Sum is accumulated in f64 to
/// keep precision when n is large; the divide writes back as f32.
fn row_stochastize(raw: &[EdgeWeight], n: usize) -> Vec<EdgeWeight> {
    let mut adj = raw.to_vec();
    for i in 0..n {
        let row_start = i * n;
        let row_sum: f64 = adj[row_start..row_start + n].iter().map(|&w| w as f64).sum();
        if row_sum > 0.0 {
            let inv = (1.0 / row_sum) as EdgeWeight;
            for j in 0..n {
                adj[row_start + j] *= inv;
            }
        }
    }
    adj
}

/// Build a CSR representation of the transpose of the dense adjacency:
/// rows of Wᵀ == columns of W. The hot spreading-activation loop reduces
/// `next[j] = Σ_i W[i][j] · f(a[i])` which is exactly a row-sweep in Wᵀ.
///
/// Returns `(row_ptr[n+1], col_idx[nnz], values[nnz])`.
fn build_csr_transpose(adj: &[EdgeWeight], n: usize) -> (Vec<usize>, Vec<usize>, Vec<EdgeWeight>) {
    if n == 0 {
        return (vec![0], Vec::new(), Vec::new());
    }
    let mut col_counts = vec![0usize; n];
    for i in 0..n {
        let row = &adj[i * n..(i + 1) * n];
        for (j, &w) in row.iter().enumerate() {
            if w > 0.0 { col_counts[j] += 1; }
        }
    }
    let mut row_ptr = vec![0usize; n + 1];
    for j in 0..n {
        row_ptr[j + 1] = row_ptr[j] + col_counts[j];
    }
    let nnz = row_ptr[n];
    let mut col_idx = vec![0usize; nnz];
    let mut values: Vec<EdgeWeight> = vec![0.0; nnz];
    let mut fill    = vec![0usize; n];
    for i in 0..n {
        let row = &adj[i * n..(i + 1) * n];
        for (j, &w) in row.iter().enumerate() {
            if w > 0.0 {
                let p = row_ptr[j] + fill[j];
                col_idx[p] = i;
                values[p]  = w;
                fill[j] += 1;
            }
        }
    }
    (row_ptr, col_idx, values)
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
        // f32 mantissa ≈ 7 digits; tolerance is loosened to match the storage precision.
        assert!((g.adj(0, 1) - 1.0 / 3.0).abs() < 1e-6);
        assert!((g.adj(0, 2) - 2.0 / 3.0).abs() < 1e-6);
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
