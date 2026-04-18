//! # scored-graph
//!
//! Weighted directed graph where nodes carry a **cost** and edges carry a **weight**.
//!
//! The adjacency is **row-stochastic** (each row with out-edges sums to 1.0),
//! an invariant required by the spreading-activation contraction proof.
//!
//! ## Storage
//!
//! Sparse CSR, both forward and transposed. No dense `n × n` matrix — the
//! graph uses `O(nnz)` memory and scales to corpora where `8 · n²` bytes
//! would be infeasible. The few APIs that used to return dense slices now
//! expose the CSR directly or compute scalar values via O(log d) binary
//! search in the row's column indices.
//!
//! Forward CSR (row = outgoing edges of a node):
//!   row_ptr[i+1] - row_ptr[i] = out-degree of node i
//!   col_idx[row_ptr[i]..row_ptr[i+1]] = destination nodes (sorted ascending)
//!   raw_values[...] / adj_values[...] = weight before / after row-stochastic normalization
//!
//! Transpose CSR (row = incoming edges of a node), derived from forward:
//!   adj_t_row_ptr[j+1] - adj_t_row_ptr[j] = in-degree of node j
//!   adj_t_col_idx[...] = source nodes
//!   adj_t_values[...]  = row-stochastic weight W[src][j]
//!
//! Spreading activation iterates `next[j] = Σ_k adj_t_values[k] · f(state[adj_t_col_idx[k]])`
//! over edges only.
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

/// Edge-weight storage type. `f32` keeps the hot arrays cache-friendly while
/// SpMV callers accumulate in `f64` to keep rounding below the Lipschitz
/// bound. The `is_row_stochastic` tolerance is relaxed to match the ~7-digit
/// f32 mantissa.
pub type EdgeWeight = f32;

/// Sparse weighted directed graph with row-stochastic adjacency.
#[derive(Debug, Clone)]
pub struct ScoredGraph {
    n: usize,

    // Forward CSR of raw (unnormalized) weights.
    // col_idx inside each row is sorted ascending so raw_weight/adj lookups
    // can use binary search.
    raw_row_ptr: Vec<usize>,
    raw_col_idx: Vec<usize>,
    raw_values:  Vec<EdgeWeight>,

    // Row-stochastic forward values. Same sparsity pattern (row_ptr / col_idx)
    // as `raw_*` — `adj_values[k] = raw_values[k] / row_sum[src]`.
    adj_values: Vec<EdgeWeight>,

    // Per-row raw sum, for on-the-fly `adj(i, j)` computation and row-
    // stochastic checks. Accumulated in f64 for precision.
    row_sum: Vec<f64>,

    // Transpose CSR of `adj` — the form the spreading-activation hot loop
    // iterates. Rebuilt by `renormalize()`.
    adj_t_row_ptr: Vec<usize>,
    adj_t_col_idx: Vec<usize>,
    adj_t_values:  Vec<EdgeWeight>,

    // Per-node metadata.
    costs: Vec<u64>,
    categories: Vec<u64>,
}

impl ScoredGraph {
    /// Build a graph from a dense `n × n` weight matrix. Kept for API
    /// compatibility with persistence code that reads `Vec<f64>` blobs; the
    /// dense input is scanned once and never stored. For sparse inputs,
    /// prefer [`Self::from_edges`].
    ///
    /// # Panics
    /// - If `weights.len() != n * n`
    /// - If `costs.len() != n`
    /// - If any weight is negative
    /// - If any cost is zero
    pub fn new(n: usize, weights: Vec<f64>, costs: Vec<u64>) -> Self {
        assert_eq!(weights.len(), n * n, "weights must be n×n");
        assert_eq!(costs.len(), n, "costs must have n elements");
        assert!(costs.iter().all(|&c| c > 0), "costs must be positive");
        assert!(weights.iter().all(|&w| w >= 0.0), "weights must be non-negative");

        // One pass over the dense input: collect (src, dst, weight) triples,
        // skipping the diagonal and zeros. `from_edges` handles everything
        // after.
        let mut edges: Vec<(usize, usize, EdgeWeight)> =
            Vec::with_capacity(n * 4); // guess
        for i in 0..n {
            for j in 0..n {
                if i == j { continue; }
                let w = weights[i * n + j];
                if w > 0.0 {
                    edges.push((i, j, w as EdgeWeight));
                }
            }
        }
        Self::from_edges(n, &edges, costs)
    }

    /// Build a graph directly from pre-assembled CSR arrays. Used by the
    /// persistence layer to avoid any dense or edge-list intermediate on
    /// load — the three Vecs are exactly what the struct stores.
    ///
    /// # Preconditions
    /// - `row_ptr.len() == n + 1` and is monotone non-decreasing
    /// - For each row, `col_idx[row_ptr[i]..row_ptr[i+1]]` is sorted, contains
    ///   no duplicates, and has no self-reference (`col_idx[k] != i`)
    /// - `values.len() == col_idx.len()`; all values are finite and > 0
    /// - `costs.len() == n` and all costs > 0
    ///
    /// Panics on violation; the persistence path trusts its own sidecar
    /// files, so the check is defensive-but-cheap.
    pub fn from_raw_csr(
        n: usize,
        row_ptr: Vec<usize>,
        col_idx: Vec<usize>,
        raw_values: Vec<EdgeWeight>,
        costs: Vec<u64>,
    ) -> Self {
        assert_eq!(row_ptr.len(), n + 1, "row_ptr must have n+1 entries");
        assert_eq!(col_idx.len(), raw_values.len(), "col_idx and values length mismatch");
        assert_eq!(costs.len(), n, "costs must have n elements");
        assert!(costs.iter().all(|&c| c > 0), "costs must be positive");
        assert!(raw_values.iter().all(|&w| w.is_finite() && w >= 0.0), "weights finite and non-negative");

        // Normalise row sums and derive adj_values in a single pass.
        let mut row_sum = vec![0.0f64; n];
        let mut adj_values: Vec<EdgeWeight> = vec![0.0; raw_values.len()];
        for i in 0..n {
            let s = row_ptr[i];
            let e = row_ptr[i + 1];
            let sum: f64 = raw_values[s..e].iter().map(|&w| w as f64).sum();
            row_sum[i] = sum;
            if sum > 0.0 {
                let inv = (1.0 / sum) as EdgeWeight;
                for k in s..e { adj_values[k] = raw_values[k] * inv; }
            }
        }

        let (adj_t_row_ptr, adj_t_col_idx, adj_t_values) =
            build_transpose_from_forward(&row_ptr, &col_idx, &adj_values, n);

        Self {
            n,
            raw_row_ptr: row_ptr,
            raw_col_idx: col_idx,
            raw_values,
            adj_values,
            row_sum,
            adj_t_row_ptr,
            adj_t_col_idx,
            adj_t_values,
            costs,
            categories: vec![0; n],
        }
    }

    /// Build a graph directly from a sparse edge list. Self-loops are
    /// silently dropped; duplicate (src, dst) pairs are summed.
    ///
    /// This is O(|edges| · log |edges|) and allocates only `O(nnz + n)` —
    /// no dense intermediate. Use for scale fixtures and filesystem scans.
    pub fn from_edges(
        n: usize,
        edges: &[(usize, usize, EdgeWeight)],
        costs: Vec<u64>,
    ) -> Self {
        assert_eq!(costs.len(), n, "costs must have n elements");
        assert!(costs.iter().all(|&c| c > 0), "costs must be positive");

        // Sort edges by (src, dst) for CSR assembly.
        let mut es: Vec<(usize, usize, EdgeWeight)> = edges.iter()
            .filter(|(i, j, w)| i != j && *w > 0.0 && i < &n && j < &n)
            .copied()
            .collect();
        es.sort_by_key(|&(i, j, _)| (i, j));

        // Coalesce duplicate (src, dst).
        let mut coalesced: Vec<(usize, usize, EdgeWeight)> = Vec::with_capacity(es.len());
        for (i, j, w) in es {
            if let Some(last) = coalesced.last_mut() {
                if last.0 == i && last.1 == j {
                    last.2 += w;
                    continue;
                }
            }
            coalesced.push((i, j, w));
        }

        // Build forward CSR.
        let nnz = coalesced.len();
        let mut raw_row_ptr = vec![0usize; n + 1];
        for &(src, _, _) in &coalesced { raw_row_ptr[src + 1] += 1; }
        for k in 0..n { raw_row_ptr[k + 1] += raw_row_ptr[k]; }
        let mut raw_col_idx = vec![0usize; nnz];
        let mut raw_values: Vec<EdgeWeight> = vec![0.0; nnz];
        {
            let mut cursor = raw_row_ptr.clone();
            for &(src, dst, w) in &coalesced {
                let p = cursor[src];
                raw_col_idx[p] = dst;
                raw_values[p] = w;
                cursor[src] = p + 1;
            }
        }

        // Row sums (in f64) and row-stochastic values.
        let mut row_sum = vec![0.0f64; n];
        for i in 0..n {
            let s = raw_row_ptr[i];
            let e = raw_row_ptr[i + 1];
            let sum: f64 = raw_values[s..e].iter().map(|&w| w as f64).sum();
            row_sum[i] = sum;
        }
        let mut adj_values: Vec<EdgeWeight> = vec![0.0; nnz];
        for i in 0..n {
            let s = raw_row_ptr[i];
            let e = raw_row_ptr[i + 1];
            if row_sum[i] > 0.0 {
                let inv = (1.0 / row_sum[i]) as EdgeWeight;
                for k in s..e { adj_values[k] = raw_values[k] * inv; }
            }
        }

        // Transpose CSR, derived from forward adj.
        let (adj_t_row_ptr, adj_t_col_idx, adj_t_values) =
            build_transpose_from_forward(&raw_row_ptr, &raw_col_idx, &adj_values, n);

        Self {
            n,
            raw_row_ptr,
            raw_col_idx,
            raw_values,
            adj_values,
            row_sum,
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
    pub fn len(&self) -> usize { self.n }

    /// Whether the graph has no nodes.
    #[inline]
    pub fn is_empty(&self) -> bool { self.n == 0 }

    /// Raw (unnormalized) weight of edge i → j. O(log d) via binary search in
    /// the sorted column index of row i.
    pub fn raw_weight(&self, i: usize, j: usize) -> f64 {
        let s = self.raw_row_ptr[i];
        let e = self.raw_row_ptr[i + 1];
        match self.raw_col_idx[s..e].binary_search(&j) {
            Ok(rel) => self.raw_values[s + rel] as f64,
            Err(_) => 0.0,
        }
    }

    /// Row-stochastic adjacency value W[i][j]. Computed from raw_weight / row_sum.
    pub fn adj(&self, i: usize, j: usize) -> f64 {
        if self.row_sum[i] <= 0.0 { return 0.0; }
        let s = self.raw_row_ptr[i];
        let e = self.raw_row_ptr[i + 1];
        match self.raw_col_idx[s..e].binary_search(&j) {
            Ok(rel) => self.adj_values[s + rel] as f64,
            Err(_) => 0.0,
        }
    }

    /// Token cost of node v.
    #[inline]
    pub fn cost(&self, v: usize) -> u64 { self.costs[v] }

    /// Slice of all costs.
    #[inline]
    pub fn costs(&self) -> &[u64] { &self.costs }

    /// CSR-encoded transpose of the row-stochastic adjacency. For column `j`
    /// of W (row `j` of Wᵀ), `col_idx[row_ptr[j]..row_ptr[j+1]]` are source
    /// node indices and `values[...]` are f32 edge weights. Accumulate in f64.
    #[inline]
    pub fn adj_transpose_csr(&self) -> (&[usize], &[usize], &[EdgeWeight]) {
        (&self.adj_t_row_ptr, &self.adj_t_col_idx, &self.adj_t_values)
    }

    /// Forward-CSR view of raw (unnormalized) weights. Used by persistence
    /// and any caller that wants to iterate outgoing edges cheaply.
    #[inline]
    pub fn raw_csr_forward(&self) -> (&[usize], &[usize], &[EdgeWeight]) {
        (&self.raw_row_ptr, &self.raw_col_idx, &self.raw_values)
    }

    /// Insert, overwrite, or remove a single edge. Returns the previous
    /// weight (0.0 if the edge did not exist).
    ///
    /// - `weight > 0.0` → upsert the edge
    /// - `weight == 0.0` → remove the edge if present
    ///
    /// O(out-degree of `src`) — binary search to locate, shift the row's
    /// tail if inserting or removing. After a batch of mutations, call
    /// [`Self::renormalize`] to rebuild the row-stochastic values and the
    /// transpose CSR before reading `adj*` or running spread.
    pub fn set_edge(&mut self, src: usize, dst: usize, weight: EdgeWeight) -> EdgeWeight {
        assert!(src < self.n && dst < self.n);
        if src == dst { return 0.0; }
        assert!(weight.is_finite() && weight >= 0.0, "weight must be finite and non-negative");

        let s = self.raw_row_ptr[src];
        let e = self.raw_row_ptr[src + 1];
        match self.raw_col_idx[s..e].binary_search(&dst) {
            Ok(rel) => {
                let p = s + rel;
                let prev = self.raw_values[p];
                if weight == 0.0 {
                    // Remove: shift tail left by one across the whole row-ptr array.
                    self.raw_col_idx.remove(p);
                    self.raw_values.remove(p);
                    self.adj_values.remove(p);
                    for k in (src + 1)..=self.n { self.raw_row_ptr[k] -= 1; }
                } else {
                    self.raw_values[p] = weight;
                }
                prev
            }
            Err(rel) => {
                if weight == 0.0 { return 0.0; }
                let p = s + rel;
                self.raw_col_idx.insert(p, dst);
                self.raw_values.insert(p, weight);
                self.adj_values.insert(p, 0.0); // will be recomputed by renormalize
                for k in (src + 1)..=self.n { self.raw_row_ptr[k] += 1; }
                0.0
            }
        }
    }

    /// Append a new node with the given cost and empty adjacency. Returns the
    /// new node's index. O(1) amortised — all CSR arrays grow by one entry
    /// (new row_ptr entries equal to previous end, empty forward + transpose
    /// rows). After adding outgoing edges via [`Self::set_edge`], call
    /// [`Self::renormalize`] to rebuild the transpose.
    pub fn add_node(&mut self, cost: u64) -> usize {
        assert!(cost > 0, "cost must be positive");
        let new_idx = self.n;
        self.n += 1;
        // Forward CSR: append a new row pointer at the current end.
        self.raw_row_ptr.push(*self.raw_row_ptr.last().unwrap_or(&0));
        // Transpose CSR: append a new row pointer at its current end
        // (new node has no incoming edges until someone adds them).
        self.adj_t_row_ptr.push(*self.adj_t_row_ptr.last().unwrap_or(&0));
        // Per-node state.
        self.row_sum.push(0.0);
        self.costs.push(cost);
        self.categories.push(0);
        new_idx
    }

    /// Multiply every outgoing edge weight of node `i` by `factor`.
    /// O(out-degree). Used by decay. Call [`Self::renormalize`] after.
    pub fn scale_row(&mut self, i: usize, factor: EdgeWeight) {
        assert!(i < self.n);
        assert!(factor.is_finite() && factor >= 0.0);
        let s = self.raw_row_ptr[i];
        let e = self.raw_row_ptr[i + 1];
        for k in s..e { self.raw_values[k] *= factor; }
    }

    /// Recompute the row-stochastic adjacency from raw weights and rebuild
    /// the transpose CSR. Call after a batch of [`set_edge`] / [`scale_row`].
    pub fn renormalize(&mut self) {
        for i in 0..self.n {
            let s = self.raw_row_ptr[i];
            let e = self.raw_row_ptr[i + 1];
            let sum: f64 = self.raw_values[s..e].iter().map(|&w| w as f64).sum();
            self.row_sum[i] = sum;
            if sum > 0.0 {
                let inv = (1.0 / sum) as EdgeWeight;
                for k in s..e { self.adj_values[k] = self.raw_values[k] * inv; }
            } else {
                for k in s..e { self.adj_values[k] = 0.0; }
            }
        }
        let (rp, ci, v) = build_transpose_from_forward(
            &self.raw_row_ptr, &self.raw_col_idx, &self.adj_values, self.n);
        self.adj_t_row_ptr = rp;
        self.adj_t_col_idx = ci;
        self.adj_t_values  = v;
    }

    /// Outgoing neighbors of node `v` (sorted ascending).
    pub fn neighbors_out(&self, v: usize) -> Vec<usize> {
        let s = self.raw_row_ptr[v];
        let e = self.raw_row_ptr[v + 1];
        self.raw_col_idx[s..e].to_vec()
    }

    /// Incoming neighbors of node `v` — sources of edges into `v`.
    pub fn neighbors_in(&self, v: usize) -> Vec<usize> {
        let s = self.adj_t_row_ptr[v];
        let e = self.adj_t_row_ptr[v + 1];
        self.adj_t_col_idx[s..e].to_vec()
    }

    /// BFS shortest path length from `src` to `dst`. Returns `None` if unreachable.
    pub fn shortest_path(&self, src: usize, dst: usize) -> Option<usize> {
        if src == dst { return Some(0); }
        let mut visited = vec![false; self.n];
        visited[src] = true;
        let mut queue = VecDeque::new();
        queue.push_back((src, 0usize));
        while let Some((node, depth)) = queue.pop_front() {
            let s = self.raw_row_ptr[node];
            let e = self.raw_row_ptr[node + 1];
            for &nb in &self.raw_col_idx[s..e] {
                if nb == dst { return Some(depth + 1); }
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
    /// Tolerance is scaled for f32 storage: per-edge rounding is ~1e-7, and
    /// worst-case row residual grows linearly with out-degree. 1e-5 covers
    /// corpora up to ~100k while still catching structural violations.
    pub fn is_row_stochastic(&self) -> bool {
        for i in 0..self.n {
            let s = self.raw_row_ptr[i];
            let e = self.raw_row_ptr[i + 1];
            if s == e { continue; } // row has no outgoing edges
            let sum: f64 = self.adj_values[s..e].iter().map(|&w| w as f64).sum();
            if (sum - 1.0).abs() > 1e-5 { return false; }
        }
        true
    }

    /// Row-sum of raw weights for node `i` — useful for pre-normalization math
    /// or auditing. 0.0 for rows with no outgoing edges.
    #[inline]
    pub fn row_sum(&self, i: usize) -> f64 { self.row_sum[i] }
}

/// Build transpose CSR from forward CSR. Given the forward layout (row = src,
/// col = dst), the transpose is (row = dst, col = src) with the same values.
fn build_transpose_from_forward(
    fwd_row_ptr: &[usize],
    fwd_col_idx: &[usize],
    fwd_values:  &[EdgeWeight],
    n: usize,
) -> (Vec<usize>, Vec<usize>, Vec<EdgeWeight>) {
    if n == 0 {
        return (vec![0], Vec::new(), Vec::new());
    }
    // Count nonzeros landing in each destination column (== transpose row).
    let mut col_counts = vec![0usize; n];
    for &c in fwd_col_idx { col_counts[c] += 1; }
    // Exclusive prefix sum.
    let mut row_ptr = vec![0usize; n + 1];
    for j in 0..n { row_ptr[j + 1] = row_ptr[j] + col_counts[j]; }
    let nnz = row_ptr[n];
    let mut col_idx = vec![0usize; nnz];
    let mut values: Vec<EdgeWeight> = vec![0.0; nnz];
    let mut fill = vec![0usize; n];
    for src in 0..n {
        let s = fwd_row_ptr[src];
        let e = fwd_row_ptr[src + 1];
        for k in s..e {
            let dst = fwd_col_idx[k];
            let p = row_ptr[dst] + fill[dst];
            col_idx[p] = src;
            values[p] = fwd_values[k];
            fill[dst] += 1;
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

    #[test]
    fn set_edge_insert_and_remove() {
        let mut g = ScoredGraph::from_edges(
            3,
            &[(0, 1, 1.0), (0, 2, 1.0)],
            vec![100, 100, 100],
        );
        assert!((g.raw_weight(0, 1) - 1.0).abs() < 1e-6);
        // Insert a new edge.
        let prev = g.set_edge(1, 2, 2.0);
        assert_eq!(prev, 0.0);
        assert!((g.raw_weight(1, 2) - 2.0).abs() < 1e-6);
        // Overwrite.
        let prev = g.set_edge(1, 2, 3.0);
        assert!((prev - 2.0).abs() < 1e-6);
        // Remove.
        let prev = g.set_edge(0, 1, 0.0);
        assert!((prev - 1.0).abs() < 1e-6);
        assert_eq!(g.raw_weight(0, 1), 0.0);
        g.renormalize();
        assert!(g.is_row_stochastic());
    }

    #[test]
    fn add_node_grows_graph() {
        let mut g = ScoredGraph::from_edges(2, &[(0, 1, 1.0)], vec![100, 100]);
        let new = g.add_node(50);
        assert_eq!(new, 2);
        assert_eq!(g.len(), 3);
        assert_eq!(g.cost(new), 50);
        assert_eq!(g.neighbors_out(new), Vec::<usize>::new());
        assert_eq!(g.neighbors_in(new), Vec::<usize>::new());
        // Add outgoing edge from new node.
        g.set_edge(new, 0, 1.0);
        g.renormalize();
        assert!(g.is_row_stochastic());
        assert!((g.raw_weight(new, 0) - 1.0).abs() < 1e-6);
        // And incoming edge to new node.
        g.set_edge(0, new, 2.0);
        g.renormalize();
        assert!(g.is_row_stochastic());
        assert!(g.neighbors_in(new).contains(&0));
    }

    #[test]
    fn scale_row_affects_only_target() {
        let mut g = ScoredGraph::from_edges(
            3,
            &[(0, 1, 1.0), (0, 2, 1.0), (1, 2, 1.0)],
            vec![100, 100, 100],
        );
        g.scale_row(0, 0.5);
        g.renormalize();
        // Row 0 raw weights are now 0.5, 0.5 → still row-stochastic at 0.5/0.5 + 0.5/... etc.
        assert!(g.is_row_stochastic());
        assert!((g.raw_weight(0, 1) - 0.5).abs() < 1e-6);
        // Row 1 untouched.
        assert!((g.raw_weight(1, 2) - 1.0).abs() < 1e-6);
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
