# cowiki

Composition layer. ~80 lines that sequence the primitives into
`retrieve()` and `maintain()`.

*Reference skeleton — expanded content planned for the next
documentation slice.*

## Public API

```rust
use cowiki::{retrieve, maintain};
```

| signature | purpose |
|---|---|
| `fn retrieve(wiki: &WikiBackend, query: &str, budget: u64) -> QueryResult` | ignite → spread → select |
| `fn maintain(wiki: &mut WikiBackend) -> HealthReport` | rem_cycle wrapper + persist |

## Invariants

- `retrieve` never mutates state.
- `maintain` always leaves the graph row-stochastic and all
  `ScoredGraph` invariants preserved (guaranteed by the
  primitives it composes).

## Composition rule

New provable properties must live in one of the primitive crates.
New filesystem or HTTP concerns must live in `wiki-backend` or
`cowiki-server`. This crate stays thin. If logic accumulates here,
it's a smell — push it down or up.

<!-- TODO: full retrieve() source walkthrough, per-step timing
     contribution, why retrieve/maintain are the only two entry
     points at this layer. -->
