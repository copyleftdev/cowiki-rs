# Architecture

The workspace is a bottom-up stack of eleven crates. Dependencies
point downward; a higher crate can call into lower ones but not
vice versa.

```text
              cowiki-server  gauntlet  (process layer, test layer)
                    │            │
              wiki-backend       │     (corpus I/O, persistence)
                    │            │
                  cowiki         │     (retrieve / maintain glue)
                    │            │
         ┌─────────┬┴┬──────────┬┴─────────┐
         ▼         ▼ ▼          ▼          ▼
   scored-graph spread budget-knap temporal-graph chunk-quality
                                (primitive layer)
```

Plus two supporting crates outside the dependency graph:

- **cl-ingest** — CourtListener bulk-data ingestors. Standalone
  binaries; don't depend on the engine, only consumed by
  operators running ingest.
- **seed-corpus** — synthetic fixtures (ba-N-K, clique-N, star-N)
  for the scale-probe and gauntlet harnesses. Test-only.

## Primitive layer

The five bottom crates are **primitives** — each proves one
mathematical property and exposes a narrow API. Every property
shows up as both a proptest in the crate and a runtime assertion
in the `gauntlet` + `runtime_audit` harnesses.

| crate | what it owns | key property |
|---|---|---|
| [scored-graph](../primitives/scored-graph.md) | row-stochastic directed graph, CSR forward + transpose | row sums = 1, no self-loops, non-negative, finite |
| [spread](../primitives/spread.md) | the iteration | \\(d \cdot L < 1\\) contraction → unique fixed point |
| [budget-knap](../primitives/budget-knap.md) | 0/1 knapsack selection | ≥½-OPT vs DP-optimal |
| [temporal-graph](../primitives/temporal-graph.md) | REM-cycle state + decay/prune/dream | preserves all scored-graph invariants under mutation |
| [chunk-quality](../primitives/chunk-quality.md) | evaluation metrics | none — it measures, it doesn't enforce |

A caller who wants to use cowiki-rs's math without its filesystem
layer can depend on these five crates directly and skip everything
above.

## Glue layer

Three crates that turn primitives into a working pipeline:

| crate | what it owns |
|---|---|
| [cowiki](../glue/cowiki.md) | retrieve() = ignite → spread → select; maintain() = rem_cycle wrapper |
| [wiki-backend](../glue/wiki-backend.md) | filesystem scan, `[[links]]` parse, TF-IDF index, SQLite + CSR sidecar persistence |
| [cowiki-server](../glue/cowiki-server.md) | Axum HTTP server, SSR routes, simulate SSE stream, UI static serving |

**Rule.** New provable properties go into the primitive layer; new
filesystem or HTTP concerns go into the glue layer. The `cowiki`
crate is deliberately thin (~80 lines) and should stay that way —
if logic ends up there, it either belongs in a primitive or in
`wiki-backend`.

## Data flow

```text
  Markdown files  ──scan──▶  PageMeta[]  ──build──▶  ScoredGraph
  on disk                    TfIdfIndex              + persistence
                                                        │
                                                        ▼
  Query string   ──ignite──▶  a⁰  ──spread──▶  a*  ──select──▶  Results
                              (TF-IDF)         (iter)     (knapsack)
```

Corpus changes (`create_page`, `update_page`) push back through
`wiki-backend` into incremental updates on the graph and TF-IDF
index, then persist. Maintenance (`/api/maintain`) runs the REM
cycle and persists.

## Test layers

| layer | where | style | count |
|---|---|---|---|
| Unit + proptest | each primitive's `src/lib.rs` under `#[cfg(test)]` | property-based invariants | 22 |
| Gauntlet | `crates/gauntlet/src/*.rs` | adversarial: pathological topologies, IEEE-754 edge cases | 41 |
| Backend VOPR | `crates/wiki-backend/tests/vopr.rs` | deterministic seeded simulation over filesystem + SQLite | 9 |
| Python proof | `proof/tests/*.py` | hypothesis replicates claims from `PROOF.md` | 37 |
| **Total** | | | **133 (+ 37 Python)** |

A pass across all layers is the shipping contract. Tests run
locally in under 60 seconds on a modern box; no network, no
managed dependencies.

## Further reading

- [scored-graph](../primitives/scored-graph.md) is the first
  primitive to read if you want to understand the engine from the
  bottom up.
- [cowiki-server](../glue/cowiki-server.md) is the first to read
  if you're running an instance and want to understand what each
  HTTP endpoint touches.
- [Case Study: SCOTUS Explorer](../case-study/premise.md) is the
  first to read if you want to understand why any of this looks
  the way it does — it's the production use case that drove most
  of the recent design decisions.
