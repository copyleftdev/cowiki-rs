# temporal-graph

REM-inspired maintenance cycle: decay edge weights, prune inert
nodes, synthesize new edges from co-activation. Called from
`/api/maintain` on a cadence chosen by the operator.

## Public API

```rust
use temporal_graph::{
    TemporalState, RemConfig, HealthReport,
    decay, prune_candidates, dream_candidates, rem_cycle, graph_health,
};
```

### Types

```rust
pub struct TemporalState {
    pub activations: Vec<f64>,   // last post-spread activations
    pub last_touched: Vec<u64>,  // clock at last activation
    pub alive: Vec<bool>,        // pruned → false
    pub clock: u64,              // monotonic, +=1 per query
}

pub struct RemConfig {
    pub decay_rate: f64,                 // default 0.01
    pub prune_threshold: f64,            // default 1e-6
    pub prune_window: usize,             // default 100 queries
    pub dream_coactivation_threshold: f64,  // default 0.7
    pub dream_max_edges: usize,          // default 50
    pub dream_edge_weight: f64,          // default 0.5
}

pub struct HealthReport {
    pub health: f64,                 // 0..1
    pub pruned: Vec<usize>,
    pub dreamed_edges: Vec<(usize, usize)>,
}
```

### Constructors and helpers

```rust
impl TemporalState {
    pub fn new(n: usize) -> Self;
    pub fn alive_count(&self) -> usize;
    pub fn recency(&self, v: usize) -> u64;   // clock - last_touched[v]
}
```

### Cycle operations

| signature | effect |
|---|---|
| `fn decay(graph: &mut ScoredGraph, state: &TemporalState, decay_rate: f64)` | Scales every alive row by `(1 - decay_rate)`, then renormalizes. |
| `fn prune_candidates(state: &TemporalState, threshold: f64, window: usize) -> Vec<usize>` | Returns nodes with `activation < threshold` AND `recency > window`. |
| `fn dream_candidates<F>(graph, state, config, coactivation_fn: F) -> Vec<(usize, usize, f64)>` | Returns up to `dream_max_edges` edge proposals above the coactivation threshold. |
| `fn rem_cycle<F>(graph, state, config, coactivation_fn: F) -> HealthReport` | Composes decay + prune + dream in one call. |
| `fn graph_health(graph, state, config) -> f64` | Read-only health score, computable without mutating. |

## Invariants

<div class="claim">

**After every `rem_cycle`:**

1. Graph remains row-stochastic (every `decay` call ends with
   `renormalize()`).
2. Pruned nodes keep their `activations` but have `alive = false`.
3. Dreamed edges respect all `ScoredGraph` invariants: no
   self-loops, non-negative, finite, host row renormalized to
   sum to 1.
4. Total edge count is non-decreasing — dream can only add;
   prune marks dead without removing storage.

</div>

## Defaults and their cadence

Under defaults (`decay_rate=0.01`, `prune_threshold=1e-6`,
`prune_window=100`), an unused edge decays to

- \\(0.99^{100} \approx 0.366\\) after 100 queries,
- \\(0.99^{500} \approx 0.007\\) after 500 queries (below typical
  prune threshold).

That's the intuition: an edge that contributes nothing for ~a
week of active use fades to zero and stops influencing rankings.
Edges that carry traffic keep being boosted (implicitly, by
other rows being weakened relative to them).

## Examples

### Run a maintenance cycle

```rust
use temporal_graph::{TemporalState, RemConfig, rem_cycle};

let mut state = TemporalState::new(graph.len());
// …populate state.activations and state.last_touched from recent queries…

let config = RemConfig::default();
let report = rem_cycle(&mut graph, &mut state, &config, |u, v| {
    // caller-supplied: return a coactivation score for node pair (u, v)
    wiki_backend::rem::coactivation(u, v)
});

println!("health={:.3}  pruned={}  dreamed={}",
    report.health, report.pruned.len(), report.dreamed_edges.len());
```

### Read-only health check

```rust
let h = graph_health(&graph, &state, &config);
// 0.3–0.7 is typical for a healthy corpus.
```

## The coactivation callback

`dream_candidates` takes a closure `Fn(usize, usize) -> f64`
rather than embedding a coactivation field in `TemporalState`.
This keeps the primitive decoupled from *how* coactivation is
computed. In cowiki-rs's stack, `wiki-backend::rem` maintains a
recent-query buffer and returns the fraction of recent queries
that activated both nodes simultaneously above a threshold.

## Notes

<div class="aside">

**Pruning marks, doesn't remove.** Pruned nodes keep their slot
in the CSR, their raw weights, their activations — just `alive =
false`. Downstream (spread, knapsack) treats `!alive` as
zero-flow, zero-selectable. Compacting would require reindexing
every subsequent operation, which we don't want on a maintenance
cycle.

</div>

<div class="postmortem">

**6,637 dreamed edges into SCOTUS on one cycle.** The first
`/api/maintain` call on the 10k SCOTUS corpus synthesized 6,637
new edges — ~5% of the initial 127k. Large in absolute terms, but
under the bound for "evolves the graph, doesn't restructure it."
The landmark cases didn't move in rankings.

</div>

## Persistence

`TemporalState` is serialized alongside the graph in
`.cowiki/engine.db`; round-trip fidelity is tested by
`save_reload_roundtrip` in the runtime audit. After a restart,
`clock` is preserved exactly — decay progress is not reset.

## Proof obligations

- `decay_preserves_row_stochasticity` — after `decay`, assert
  `graph.is_row_stochastic()`.
- `prune_idempotent` — calling prune on already-pruned nodes is
  a no-op.
- `dream_respects_limits` — `dream_candidates` returns at most
  `config.dream_max_edges`, none self-loops, none duplicating
  existing edges.
- `vopr_long_horizon` — 500 random REM cycles interleaved with
  queries; asserts all four invariants hold at every step.
  Caught the f32 subnormal precision cliff at step 409; see
  [scored-graph](scored-graph.md).
