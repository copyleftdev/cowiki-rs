# scored-graph

`scored-graph` is the weighted directed graph that underpins everything.
The spreading-activation iteration runs over it. The knapsack reads
its costs. The temporal-graph crate mutates its weights. Every other
primitive in this part either produces or consumes a `ScoredGraph`.

The crate is ~500 lines. Most of those lines exist to make the
following claim true at all times:

<div class="claim">

**Claim.** For every public constructor of `ScoredGraph` and every
public mutator that preserves the structure, the resulting graph
satisfies:

1. **Row-stochastic adjacency.** For each node \\(i\\) with at least
   one outgoing edge, \\(\sum_j W_{ij} = 1\\). Nodes with no outgoing
   edges have \\(\sum_j W_{ij} = 0\\) (called *dead rows* — legal, but
   contribute no flow).
2. **No self-loops.** \\(W_{ii} = 0\\) for all \\(i\\).
3. **Non-negative weights.** \\(W_{ij} \ge 0\\) for all \\(i, j\\).
4. **Positive costs.** \\(c_i > 0\\) for all nodes \\(i\\) (needed for
   the knapsack; zero-cost nodes produce unbounded density).
5. **Finite values.** No NaN, no ±∞ anywhere in weights or costs.

</div>

These properties are the contract on which [spread](spread.md) proves
convergence. A `ScoredGraph` that violates any of them causes the
spreading iteration's contraction guarantee to fail, and downstream
tests (`is_row_stochastic`, the gauntlet crate's fixture suite) catch
that. We have seen all five of these invariants break in practice at
least once; the crate is pinned down as hard as it is because each
break produced a silent data-corruption bug somewhere else in the
stack.

## Representation

The graph is stored twice. The forward direction — "given source
\\(i\\), which nodes does it point to?" — lives in one CSR triple:

```rust
raw_row_ptr: Vec<usize>,   // length n+1
raw_col_idx: Vec<usize>,   // length nnz, sorted within each row
raw_values:  Vec<EdgeWeight>,  // length nnz, raw authored weights
adj_values:  Vec<EdgeWeight>,  // length nnz, row-stochastic-normalized weights
```

The reverse direction — "given target \\(j\\), which nodes point to it,
and with what row-stochastic weight?" — lives in a separate CSR triple:

```rust
adj_t_row_ptr: Vec<usize>,    // length n+1
adj_t_col_idx: Vec<usize>,    // length nnz, source nodes
adj_t_values:  Vec<EdgeWeight>,  // length nnz, row-stochastic weights
```

Both CSRs share the same `nnz` and the same sparsity pattern modulo
transposition. Both are kept in sync by `renormalize()`, which is
called after every mutation.

### Why two CSRs

The forward CSR answers `raw_weight(i, j)`. The transposed CSR is
used by the spreading-activation inner loop:

```rust
// Pseudocode for one iteration of spread:
for j in 0..n {
    let s = adj_t_row_ptr[j];
    let e = adj_t_row_ptr[j + 1];
    let mut acc = 0.0;
    for k in s..e {
        let i = adj_t_col_idx[k];
        let w = adj_t_values[k];       // W[i][j] row-stochastic
        acc += w * f(prev_activation[i]);
    }
    next_activation[j] = d * acc + (1.0 - d) * ignition[j];
}
```

The loop iterates *incoming* edges for each target \\(j\\). Iterating the
forward CSR would require, for each \\(j\\), finding all \\(i\\) such that
\\(j \in \texttt{col\_idx}[\texttt{row\_ptr}[i]..\texttt{row\_ptr}[i+1]]\\) —
that's an \\(O(n)\\) scan per target, \\(O(n^2)\\) overall. The transposed
CSR makes it \\(O(\texttt{in-degree}(j))\\) per target, \\(O(\texttt{nnz})\\)
overall, which is the bound we wanted.

<div class="aside">

**Aside.** For a long time the crate stored \\(W\\) as a dense
\\(n \times n\\) matrix. At \\(n = 25{,}000\\) with \\(f64\\) weights that's
5 GiB. The scale-ladder tests identified this as the single dominant
RSS cost in the pipeline, and commit `419b4c9` (*CSR-only
ScoredGraph*) collapsed it. RSS at \\(n = 25{,}000\\) fell from 11.2 GiB
to ~1 GiB.

See the postmortem for a related bug: moving to `f32` weights
introduced a precision cliff where repeated decay drove weights into
the subnormal range, producing \\(1/0 = \infty\\) in the normalization
step. Caught by `vopr_long_horizon` at step 409 with a row-stochastic
invariant violation. Fixed in commit `fe65144`.

</div>

### Row-stochastic normalization

The `raw_values` carry the *authored* weight of each edge — what the
graph's producer (wiki scanner, citation aggregator) chose to assign.
The `adj_values` are the *row-stochastic* weights used by the
iteration. They're re-derived every time the graph is constructed or
mutated:

```rust
fn normalize_row(raw_row: &[f32]) -> Vec<f32> {
    let sum: f64 = raw_row.iter().map(|&w| w as f64).sum();
    if sum == 0.0 || 1.0 / sum > f32::MAX as f64 {
        // Dead row (all zeros) OR row where subnormals would overflow.
        // Mark dead; the iteration sees zero flow out of this node.
        return vec![0.0; raw_row.len()];
    }
    let scale = (1.0 / sum) as f32;
    raw_row.iter().map(|&w| w * scale).collect()
}
```

The second branch is the precision-cliff fix from `fe65144`. Without
it, a row whose `raw_values` had been decayed into the subnormal range
(e.g. \\(\sim 10^{-40}\\)) had `sum ~ 10^{-40}`, `1/sum ~ 10^{40}`,
which overflows `f32`'s representable range, producing `Inf`. Every
`adj_value` in that row became `Inf`, the row's sum became `Inf`, and
`is_row_stochastic()` returned false. Now we detect the condition and
treat the row as dead.

## Public API

The crate exposes ~15 public methods. The important ones:

### Constructors

```rust
pub fn new(n: usize, weights: Vec<f64>, costs: Vec<u64>) -> Self
```
Build from a dense \\(n \times n\\) weights vector. Row-major, so
`weights[i*n + j]` is \\(W_{ij}\\). Panics on diagonal weights, negative
weights, or wrong length. Internally downgrades to `f32` and builds
the CSRs.

```rust
pub fn from_edges(
    n: usize,
    edges: Vec<(usize, usize, EdgeWeight)>,
    costs: Vec<u64>,
) -> Self
```
Sparse constructor, preferred when you know the edge count is
\\(\ll n^2\\). Builds CSRs directly from the edge list. Also the
entry-point used by `wiki-backend::graph::build_graph`.

```rust
pub fn from_raw_csr(
    n: usize,
    row_ptr: Vec<usize>,
    col_idx: Vec<usize>,
    values: Vec<EdgeWeight>,
    costs: Vec<u64>,
) -> Self
```
Zero-copy persistence fast-path. Takes pre-assembled CSR arrays
directly (from disk via `persist::load`) and skips both the dense-
matrix intermediate and the edge-list sort. Used by the `.cowiki/`
sidecar loader; see
[cowiki-server architecture](../part3/cowiki-server.md) for the
sidecar layout.

### Reads

```rust
pub fn raw_weight(&self, i: usize, j: usize) -> f64
pub fn adj(&self, i: usize, j: usize) -> f64
pub fn cost(&self, v: usize) -> u64
pub fn len(&self) -> usize                    // n
pub fn neighbors_out(&self, v: usize) -> Vec<usize>
pub fn neighbors_in(&self, v: usize) -> Vec<usize>
pub fn shortest_path(&self, src: usize, dst: usize) -> Option<usize>
pub fn is_row_stochastic(&self) -> bool
```

`raw_weight(i, j)` and `adj(i, j)` both do a binary search in the
forward CSR's \\(i\\)-th row. The binary search is cheap (~30 ns for
typical row widths) but does not amortize if you're iterating a lot
of edges — prefer `neighbors_out` / `neighbors_in` for those loops.

### Mutators

```rust
pub fn set_edge(&mut self, src: usize, dst: usize, weight: EdgeWeight) -> EdgeWeight
pub fn set_cost(&mut self, v: usize, cost: u64)
pub fn add_node(&mut self, cost: u64) -> usize
pub fn scale_row(&mut self, i: usize, factor: EdgeWeight)
pub fn renormalize(&mut self)
```

Each of these leaves the graph in a state where `renormalize()` must
be called before the next read. `set_edge`, `set_cost`, and
`add_node` do the renormalization implicitly. `scale_row` does not —
it's used inside the decay loop which does a batch of scales
followed by one `renormalize()`.

`renormalize()` rebuilds `adj_values` and both `adj_t_*` arrays from
`raw_values`. It's \\(O(\texttt{nnz})\\).

## The transpose sync

Every mutation triggers a rebuild of the transposed CSR. This was a
conscious choice against lazy rebuild.

Two reasons. First, eager rebuild means `is_row_stochastic()` is
always true between mutations — no "stale" window where an internal
consistency check could accidentally succeed on the forward CSR and
fail on the transposed one. Second, the transpose rebuild is cheap
compared to a single query: \\(O(\texttt{nnz})\\) versus ~30 iterations
over the full transposed CSR. In the `wiki-backend` write path, a
`create_page` call does one renormalize per call; the cost is noise
against the TF-IDF update's cost.

## Persistence: the sidecar layout

`ScoredGraph` has no `serde::Serialize` impl on purpose. The
`wiki-backend::persist` module writes it to three mmap-ready sidecar
files under `.cowiki/`:

```
.cowiki/
├── graph.row_ptr     u64 LE, (n+1) entries
├── graph.col_idx     u64 LE, nnz entries
└── graph.values      f32 LE, nnz entries
```

Each file is a fixed-width type. No framing, no headers, no parser in
between. Load is `std::fs::read` + `bytemuck::cast_slice` (or, in a
future revision, `memmap2::Mmap::map` for zero-copy).

<div class="aside">

**Aside.** The earlier persistence path wrote \\(W\\) as a single BLOB
column in SQLite. SQLite's hard cap on BLOB size is 1 GiB, which the
dense \\(n^2\\) weights hit at \\(n \approx 11{,}500\\). Commit `c45aa48`
moved to the sidecar layout. Side effect: the save path is now linear
in `nnz`, not quadratic in \\(n\\). At \\(n = 25{,}000\\) the save time
dropped from "impossible" (blob too big) to ~1 second.

</div>

## Proof obligations

What the crate's unit tests and proptests enforce, beyond the five
invariants stated above:

- `row_stochastic_after_mutation` — after every `set_edge`, `add_node`,
  or `set_cost`, re-check the five invariants.
- `renormalize_is_idempotent` — calling `renormalize()` twice gives the
  same CSR.
- `transpose_matches_forward` — the reverse CSR agrees with the
  forward CSR on every \\((i, j)\\): the sum of edges entering \\(j\\)
  equals the sum over sources of \\(W_{ij}\\).
- `shortest_path_correctness` — spot-checked against hand-computed
  distances on small graphs.

These tests live in `crates/scored-graph/src/lib.rs` under
`#[cfg(test)]`. The `gauntlet` crate adds adversarial fixtures
(pathological topologies, IEEE 754 edge cases, worst-case row sums);
those live in `crates/gauntlet/src/`.

The combined test matrix is the contract. A mutation that passes all
of it is allowed to land; a mutation that doesn't isn't.
