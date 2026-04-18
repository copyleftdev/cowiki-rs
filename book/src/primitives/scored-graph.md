# scored-graph

Row-stochastic directed graph with CSR forward + transpose
storage. Underpins every other primitive: spread reads its
transposed CSR, budget-knap reads its costs, temporal-graph
mutates its weights.

## Public API

```rust
use scored_graph::{ScoredGraph, EdgeWeight};  // EdgeWeight = f32
```

### Types

| item | purpose |
|---|---|
| `struct ScoredGraph` | The graph. Opaque; all access through methods. |
| `type EdgeWeight = f32` | Edge weight representation. |

### Constructors

| signature | use when |
|---|---|
| `ScoredGraph::new(n, weights: Vec<f64>, costs: Vec<u64>) -> Self` | You have a dense \\(n \times n\\) weights vector (row-major). Panics on invariant violation. |
| `ScoredGraph::from_edges(n, edges: Vec<(usize, usize, EdgeWeight)>, costs: Vec<u64>) -> Self` | You have a sparse edge list. Preferred for typical graph sizes. |
| `ScoredGraph::from_raw_csr(n, row_ptr, col_idx, values, costs) -> Self` | Persistence fast-path — CSR arrays already assembled on disk. Zero validation overhead beyond what is already true of CSR data. |

### Reads

| signature | complexity |
|---|---|
| `fn len(&self) -> usize` | O(1) |
| `fn is_empty(&self) -> bool` | O(1) |
| `fn raw_weight(&self, i, j) -> f64` | O(log deg) binary search in row `i` |
| `fn adj(&self, i, j) -> f64` | O(log deg) |
| `fn cost(&self, v: usize) -> u64` | O(1) |
| `fn costs(&self) -> &[u64]` | O(1) |
| `fn neighbors_out(&self, v) -> Vec<usize>` | O(out-deg), copies |
| `fn neighbors_in(&self, v) -> Vec<usize>` | O(in-deg), copies |
| `fn shortest_path(&self, src, dst) -> Option<usize>` | BFS, O(V+E) |
| `fn is_row_stochastic(&self) -> bool` | O(n) |
| `fn row_sum(&self, i) -> f64` | O(1) — cached |
| `fn raw_csr_forward(&self) -> (&[usize], &[usize], &[EdgeWeight])` | O(1), borrows |
| `fn adj_transpose_csr(&self) -> (&[usize], &[usize], &[EdgeWeight])` | O(1), borrows |

### Mutators

| signature | contract |
|---|---|
| `fn set_edge(&mut self, src, dst, weight) -> EdgeWeight` | Inserts or overwrites. Renormalizes source row. Returns previous weight (0.0 if absent). Panics on `src == dst`. |
| `fn set_cost(&mut self, v, cost: u64)` | Replaces cost. Panics if `cost == 0`. |
| `fn add_node(&mut self, cost: u64) -> usize` | Returns the new node's index. |
| `fn scale_row(&mut self, i, factor: EdgeWeight)` | Multiplies raw weights; caller must `renormalize()` before next read. |
| `fn renormalize(&mut self)` | Rebuilds `adj_values` and both `adj_t_*` arrays from `raw_values`. O(nnz). |
| `fn with_categories(self, categories: Vec<u64>) -> Self` | Builder-style attach of optional per-node category tags. |

## Invariants

<div class="claim">

**Preserved by every public constructor and mutator:**

1. **Row-stochastic.** For each node \\(i\\) with out-degree ≥ 1,
   \\(\sum_j W_{ij} = 1\\). Dead rows (out-degree 0) sum to 0.
2. **No self-loops.** \\(W_{ii} = 0\\) for all \\(i\\).
3. **Non-negative.** \\(W_{ij} \ge 0\\) for all \\(i, j\\).
4. **Positive costs.** \\(c_i > 0\\) for all \\(i\\).
5. **Finite.** No NaN, no ±∞, anywhere.

</div>

Violating any of these breaks the convergence argument in
[`spread`](spread.md). The `gauntlet` crate's fixture suite plus
`vopr_long_horizon` enforce all five under adversarial mutation
sequences.

## Representation

The graph is stored twice for \\(O(\text{nnz})\\) iteration in both
directions:

```text
Forward CSR (raw weights — what authors specified):
  raw_row_ptr: [usize; n+1]
  raw_col_idx: [usize; nnz]   sorted within each row
  raw_values:  [f32;   nnz]

Forward CSR (adj weights — row-stochastic-normalized):
  adj_values:  [f32;   nnz]   same row_ptr/col_idx as above

Transpose CSR (adj weights, inverted for incoming-edge iteration):
  adj_t_row_ptr: [usize; n+1]
  adj_t_col_idx: [usize; nnz]  sources
  adj_t_values:  [f32;   nnz]
```

The spread iteration's inner loop reads the transposed CSR:

```rust
for j in 0..n {
    let s = adj_t_row_ptr[j];
    let e = adj_t_row_ptr[j + 1];
    let mut acc = 0.0;
    for k in s..e {
        let i = adj_t_col_idx[k];
        acc += (adj_t_values[k] as f64) * threshold.apply(prev[i]);
    }
    next[j] = damp * acc + (1.0 - damp) * ignition[j];
}
```

## Examples

### Dense constructor

```rust
// 3-node graph, complete bidirectional minus self-loops.
let w = vec![
    0.0, 0.5, 0.5,   // row 0: 0→1, 0→2 each 0.5
    0.5, 0.0, 0.5,
    0.5, 0.5, 0.0,
];
let costs = vec![10, 10, 10];
let g = ScoredGraph::new(3, w, costs);
assert!(g.is_row_stochastic());
```

### Sparse constructor

```rust
let edges = vec![
    (0, 1, 1.0),
    (0, 2, 1.0),
    (1, 2, 0.3),
    (2, 0, 0.7),
];
let g = ScoredGraph::from_edges(3, edges, vec![10, 10, 10]);
// Row 0's raw weights (1.0, 1.0) normalize to (0.5, 0.5) in adj_values.
```

### Reading a neighborhood

```rust
for j in g.neighbors_out(0) {
    println!("0 → {j}  raw={:.3}  adj={:.3}", g.raw_weight(0, j), g.adj(0, j));
}
```

### Mutation

```rust
let prev = g.set_edge(0, 1, 2.0);   // overwrites; row auto-renormalizes
assert_eq!(prev, 1.0);
g.set_cost(2, 25);                  // updates cost, no graph change
```

## Notes

<div class="aside">

**Moving from dense to CSR.** At \\(n = 25{,}000\\) with f64 dense
weights the in-memory graph was 5 GiB. Commit `419b4c9` collapsed
it to CSR f32; at the same \\(n\\) it's now ~60 MiB. See [Scale
envelope](../measurements/scale-envelope.md) for the full ladder.

</div>

<div class="postmortem">

**Precision cliff on decayed weights.** After many decay cycles,
raw f32 weights can enter the subnormal range
(\\(\sim 10^{-40}\\)). Their row-sum is tiny but nonzero, so
\\(1/\text{sum}\\) in f64 is ~\\(10^{42}\\), which casts to f32 as
`inf`. Every normalized weight in the row becomes `inf`, the row's
sum becomes `inf`, `is_row_stochastic()` reports false. Caught by
`vopr_long_horizon` at step 409 in commit `fe65144`; fix checks
whether \\(1/\text{sum}\\) would overflow f32 and marks such rows
dead instead of dividing.

</div>

## Persistence

`ScoredGraph` does not impl `Serialize`. Use
[wiki-backend::persist](../glue/wiki-backend.md) which writes three
fixed-width sidecar files under `.cowiki/`:

```text
.cowiki/
├── graph.row_ptr    u64 LE, (n+1) entries
├── graph.col_idx    u64 LE, nnz entries
└── graph.values     f32 LE, nnz entries
```

Load is `std::fs::read` + `bytemuck::cast_slice`. See
[Persistence](../ops/persistence.md) for the full layout.

## Proof obligations

Tests in `crates/scored-graph/src/lib.rs::tests` enforce:

- `row_stochastic_after_mutation` — invariants hold after every
  `set_edge`, `add_node`, `set_cost`.
- `renormalize_idempotent` — `renormalize()` twice is a no-op.
- `transpose_matches_forward` — the reverse CSR sums agree with
  forward CSR sums over every \\((i, j)\\).
- `shortest_path_correctness` — spot-checked against
  hand-computed distances on small graphs.

The `gauntlet` crate adds adversarial fixtures — pathological
topologies, IEEE-754 edge cases, worst-case row sums.
