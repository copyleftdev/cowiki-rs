# temporal-graph

`temporal-graph` implements the REM-inspired maintenance cycle that
keeps a cowiki-rs graph *alive* — adjusting edge weights downward
over time, pruning nodes that have become inert, and synthesizing
new edges from co-activation patterns in recent queries. It is the
only crate in Part II whose output isn't a function purely of the
corpus: its state includes *when* each node was last touched, and
its operations depend on recent query history.

The crate is ~400 lines. Its purpose is to stop the graph from
becoming a museum — fossilized in the shape it had at
index-construction time, insensitive to what users are actually
reaching for.

## The mental model

The graph starts as a snapshot of authored edges from the corpus.
Over days or weeks of queries:

- Some edges are heavily used (spreading activation routes traffic
  through them often).
- Some edges are never used — the authors wrote them but no query
  finds them meaningful.
- Some *pairs of nodes* are co-activated by queries despite not
  being connected in the authored graph.

The REM cycle acts on all three observations:

- **Decay** reduces all edge weights slightly each cycle, so unused
  edges fade.
- **Prune** marks nodes dead that have fallen below an activation
  floor and haven't been recently touched.
- **Dream** proposes new edges between pairs of nodes that have
  co-activated recently.

The names come from the sleep-consolidation literature (Stickgold,
McClelland). We are not claiming biological realism — the analogy
gives the operations names that are easier to remember than
"weight-decay-step, sparsity-threshold-prune, co-activation-edge-
synthesis."

## TemporalState

```rust
pub struct TemporalState {
    pub activations: Vec<f64>,        // last seen post-spread activation per node
    pub last_touched: Vec<u64>,        // monotonic counter of when node was last used
    pub alive: Vec<bool>,              // pruned nodes are marked false
    pub clock: u64,                   // monotonic counter, +=1 per query
}
```

The `clock` increments once per retrieval (not per iteration). It is
not wall-clock time; the REM cycle's notions of "recent" and "old"
are framed in queries-since, not seconds-since. This keeps the system
insensitive to load patterns — a quiet week doesn't trigger more
decay than a busy week.

`last_touched[v]` is set to `clock` after every spread pass that
activates node \\(v\\) above a small threshold. `alive[v]` is `false`
when the node has been pruned; dead nodes remain in the graph (they
still have indices, slots in the CSR) but contribute no flow to
spreading activation and are never selected by the knapsack.

## RemConfig

```rust
pub struct RemConfig {
    pub decay_rate: f64,                 // default 0.01
    pub prune_threshold: f64,             // default 1e-6
    pub prune_window: usize,               // default 100 queries
    pub dream_coactivation_threshold: f64, // default 0.7
    pub dream_max_edges: usize,            // default 50 per cycle
    pub dream_edge_weight: f64,            // default 0.5
}
```

The defaults are conservative. `decay_rate = 0.01` means edge
weights multiply by 0.99 per cycle; a cycle that hasn't contributed
flow for 100 queries drops to \\(0.99^{100} \approx 0.366\\). A cycle
idle for 500 queries drops to \\(\approx 0.007\\), well below the
prune threshold. So an unused edge effectively vanishes over a week
of active use, which matches the intuition.

## decay()

```rust
pub fn decay(graph: &mut ScoredGraph, state: &TemporalState, decay_rate: f64) {
    for i in 0..graph.len() {
        if !state.alive[i] { continue; }
        graph.scale_row(i, (1.0 - decay_rate) as f32);
    }
    graph.renormalize();
}
```

Scale each row by \\(1 - \texttt{decay\_rate}\\). Renormalize once at
the end. Because scaling is row-wise and we renormalize, the *row-
stochastic* weights are unchanged per iteration — every row still
sums to 1. What changes is the *raw* weights, and specifically the
relative mass between rows. Once a row's raw sum is small enough, a
subsequent row-wise comparison (used by prune) will mark it as a
pruning candidate.

Decay is the only operation that touches every edge. On a 500k-node
graph with \\(\sim 10^6\\) edges, it takes ~5 ms. It's a single call
per REM cycle, not per query.

## prune_candidates() and prune()

```rust
pub fn prune_candidates(
    state: &TemporalState,
    threshold: f64,
    window: usize,
) -> Vec<usize>
```

Returns the list of nodes \\(v\\) such that:

- \\(\texttt{state.activations}[v] < \texttt{threshold}\\), AND
- \\(\texttt{state.clock} - \texttt{state.last\_touched}[v] > \texttt{window}\\)

Both conditions must hold: the node is activation-wise uninteresting,
AND it hasn't been hit recently. Either alone is not grounds to
prune.

The actual prune is a flip of `alive[v] = false`. No edges are
removed from the CSR; they just stop contributing, because the
temporal-aware spread function will zero out flow into and out of
dead nodes. This preserves the graph's storage layout under
mutation — the alternative (compacting the CSR) would require
reindexing every subsequent operation, which is not a cost we want
to pay on a maintenance cycle.

## dream_candidates() and dreaming

```rust
pub fn dream_candidates<F>(
    graph: &ScoredGraph,
    state: &TemporalState,
    config: &RemConfig,
    get_coactivation: F,
) -> Vec<(usize, usize, f64)>
where F: Fn(usize, usize) -> f64
```

Dreaming is the only operation that *adds* edges. For every pair of
alive nodes \\((u, v)\\) where:

- \\(u\\) and \\(v\\) are not already connected,
- a caller-supplied co-activation measure exceeds
  `dream_coactivation_threshold`,

propose a new edge with weight `dream_edge_weight`. Return the top
`dream_max_edges` such proposals, sorted by co-activation strength.

The co-activation measure is a callback, not an internal field,
because "co-activation" is a property of query history that the
temporal-graph crate doesn't want to know how to compute. In
cowiki-rs's stack it's implemented in `wiki-backend::rem`: the
co-activation score of \\((u, v)\\) is the fraction of recent queries in
which both nodes' activations exceeded a threshold simultaneously.

The dream_max_edges cap is there to prevent a single REM cycle from
restructuring the graph so aggressively that the next cycle has no
relationship to the graph the user experienced yesterday.

<div class="aside">

**Aside.** We saw 6,637 edges dreamed into the SCOTUS corpus on a
single maintenance run (see [Part IV,
Chapter 2](../part4/stubs.md)). That's a large number in absolute
terms but it's ~3% of the initial 130k edges — within the bound of
"evolves the graph, doesn't restructure it." The landmark cases
didn't move in the rankings.

</div>

## rem_cycle()

The three operations compose into `rem_cycle`:

```rust
pub fn rem_cycle<F>(
    graph: &mut ScoredGraph,
    state: &mut TemporalState,
    config: &RemConfig,
    get_coactivation: F,
) -> HealthReport
where F: Fn(usize, usize) -> f64
```

Runs decay, then prune (applying the candidates), then dream (adding
the proposals). Returns a `HealthReport` summarizing what happened:

```rust
pub struct HealthReport {
    pub health: f64,            // overall "aliveness" score, 0..1
    pub pruned: Vec<usize>,
    pub dreamed_edges: Vec<(usize, usize)>,
}
```

`health` is the ratio of alive nodes with recent `last_touched` to
total nodes. A healthy corpus is around 0.3–0.7; below 0.1 indicates
either a very small active region or a maintenance cycle being run
too often.

## graph_health()

```rust
pub fn graph_health(
    graph: &ScoredGraph,
    state: &TemporalState,
    config: &RemConfig,
) -> f64
```

Same score as above, computable without running a cycle. Exposed
via `/api/maintain`'s response body so operators can monitor health
without mutating state.

## Invariants

<div class="claim">

**Claim.** After every `rem_cycle` call:

1. The graph remains row-stochastic (enforced by `renormalize` at the
   end of `decay`).
2. Pruned nodes have `activations` preserved (we don't zero them) but
   `alive = false`.
3. Dreamed edges respect all `ScoredGraph` invariants: no self-loops,
   non-negative, finite, and the row they land in is renormalized to
   sum to 1.
4. The total edge count is non-decreasing (dream can only add, prune
   marks dead but doesn't remove).

</div>

The tests in `crates/temporal-graph/src/lib.rs` check each of these
explicitly after every `rem_cycle` call on random configurations.

## VOPR

`vopr_long_horizon` in `crates/wiki-backend/tests/vopr.rs` runs 500
REM cycles in sequence on a synthetic corpus, with random queries
between them, and checks all four invariants at every step. This
caught the precision cliff in commit `fe65144` (subnormal weights
producing \\(1/0 = \infty\\) in row normalization) at step 409 — a
bug that would not have shown up in a shorter test but which any
production `/api/maintain` loop would have hit within a few weeks.

See [Part V](../part5/end-to-end.md) for the full VOPR methodology.

## Why this crate exists at all

Skeptically: if the graph is static at index time, does cowiki-rs need
a decay/prune/dream loop? Couldn't we just re-scan the corpus on each
update?

Yes, if the corpus itself doesn't change. But the *graph* is richer
than the corpus: it contains dreamed edges that came from query
patterns, it has decayed edges that reflect actual use, and its alive
set reflects which nodes remain interesting. Rebuilding from the
corpus alone throws all of this away.

More concretely, the REM cycle lets cowiki-rs express things that
authored graphs cannot:

- *"These two pages are semantically related even though nobody has
  linked them yet."* Dream edges capture this.
- *"This page is orphaned — nobody links to it, no queries reach it,
  it should probably be archived."* Prune captures this.
- *"These five links are the real ones; the other forty are wiki
  clutter."* Decay, running over time, sorts strong signal from
  noise.

The cost is state: the system is not stateless between queries,
and persistence has to save `TemporalState` alongside the graph.
[wiki-backend::persist](../part3/wiki-backend.md) handles the
SQLite serialization; the important property is that a save/reload
round-trip preserves `last_touched` and `alive` exactly, so a
restart doesn't reset decay progress.
