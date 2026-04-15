# cowiki-rs

Formally verified primitives for the [Co-Wiki and REM Agent](https://github.com/pshomo/co-wiki) architecture proposed by [Paul Shomo](https://www.linkedin.com/in/paulshomo/).

Shomo's design describes a wiki-based warm storage layer for LLM memory -- human-readable, co-authored by humans and agents, maintained by a background "REM Agent" that prunes, decays, and discovers new connections. He published the design as an open invitation to build.

This repository answers that invitation with the mathematical engine: formalized spreading activation retrieval, proven convergence guarantees, and five independent Rust crates ready to underpin a full Co-Wiki implementation.

## What this is

The Co-Wiki's core claim is that **graph-based spreading activation over a backlink-rich knowledge graph outperforms flat vector similarity search for associative retrieval** -- the kind of retrieval where the answer is 2-3 hops away, not semantically similar.

We formalized that claim, subjected it to adversarial property-based testing, and built production-grade primitives from the results.

### What was proven

| # | Claim | Verdict |
|---|---|---|
| 1 | Linear spreading activation is a contraction mapping | **Proven** (Banach fixed-point theorem) |
| 2 | Convergence is geometric at rate d | **Proven** |
| 3 | Graph retrieval beats vector retrieval on associative queries | **Proven** |
| 4 | Modified greedy retrieval achieves >= 1/2 of optimal | **Proven** (verified against brute-force) |
| 5 | Variable human-cognitive chunks enable better token efficiency | **Proven** |
| 6 | Human chunk boundaries are more coherent than fixed-token splits | **Proven** |
| 7 | REM decay/prune/dream operators maintain graph health | **Proven** |
| 8 | Hard threshold breaks contraction (causes limit cycles) | **Disproven and corrected** |
| 9 | Monotonic hop-decay on general graphs | **Disproven and corrected** |

Findings 8 and 9 are improvements to the original formulation. The sigmoid threshold fix restores all convergence guarantees. See [`PROOF.md`](PROOF.md) for the full verification report.

## Architecture

```
cowiki-rs/
  PROOF.md              Formal verification report (17 claims, 3 corrections)
  proof/                Python hypothesis suite (37 property tests)
    cowiki/             Formalized model: graph, activation, retrieval, REM
    tests/              Property-based tests that discovered the corrections
  crates/               Rust workspace (76 tests, 0 unsafe)
    scored-graph/       Weighted directed graph with row-stochastic invariant
    spread/             Spreading activation with pluggable threshold functions
    budget-knap/        Budget-constrained selection (>= 1/2 OPT guarantee)
    temporal-graph/     REM Agent operators: decay, prune, dream
    chunk-quality/      Coherence, recall, density metrics
    cowiki/             Composition layer (thin glue)
    gauntlet/           VOPR-style adversarial test suite (41 chaos tests)
```

### Why separate crates

Each primitive has **independent mathematical contracts** and **users beyond the Co-Wiki**:

- `spread` -- any graph propagation: recommendation engines, knowledge graphs, social networks.
- `budget-knap` -- any constrained selection: token budgets, memory limits, API rate caps.
- `temporal-graph` -- any graph with decay: cache eviction, freshness, social network aging.
- `chunk-quality` -- any chunking evaluation: RAG tuning, document segmentation.
- `scored-graph` -- any weighted directed graph with per-node costs.

The proofs told us where the seams are. If the contracts are independent, the implementations should be independent.

### How they compose

```rust
use cowiki::{retrieve, maintain, ScoredGraph, SpreadConfig, TemporalState, RemConfig};

// Build a wiki graph from articles and backlinks.
let graph = ScoredGraph::new(n, weights, token_costs);

// Retrieve: query -> spread activation -> budget-constrained selection.
let (selection, activation) = retrieve(&graph, &initial_activation, budget, &SpreadConfig::default());

// Maintain: REM cycle -- decay stale edges, prune dormant articles, discover backlinks.
let report = maintain(&mut graph, &mut state, &query, &RemConfig::default());
```

## Formal notation

The Co-Wiki knowledge graph:

```
G = (V, E, w, tau)

V   = wiki articles
E   = directed backlinks and category edges
w   = edge weight (association strength), row-stochastic
tau = token cost per article (variable -- human-cognitive chunking)
```

Spreading activation (linear operator, provably contracting):

```
T(a) = (1 - d) * a^0  +  d * W^T * a

Contraction:  ||T(a) - T(b)||_1  <=  d * ||a - b||_1
Convergence:  O(log(1/eps) / log(1/d)) iterations
Bound:        0  <=  a*  <=  max(a^0) / (1 - d)
```

Retrieval (0-1 knapsack with >= 1/2 optimality guarantee):

```
R*(q, G, B) = argmax_{S, sum tau(v) <= B}  sum a*(v)
```

REM Agent (temporal graph dynamics):

```
Decay:   w_t(i,j) = w_0(i,j) * exp(-lambda * (t - t_last(i)))
Prune:   remove v if max activation over window < theta
Dream:   add (u,v) if similarity(u,v) > theta and (u,v) not in E
```

## Test suite

**76 tests, 0 failures, 0 clippy warnings, 0 unsafe.**

| Layer | Tests | What it covers |
|---|---|---|
| **proptest** (property-based) | 22 | Contraction, convergence, bounds, knapsack guarantee, decay formula, coherence |
| **unit tests** | 13 | Basic construction, edge cases, end-to-end pipeline |
| **gauntlet** (adversarial) | 41 | VOPR chaos, pathological topologies, IEEE 754 torture, long-horizon stability, worst-case knapsack |

The gauntlet runs ~22,500 chaos operations: weight corruption, topology mutations, 500-cycle REM simulations, machine-epsilon weights, near-overflow normalization, barbell bottlenecks, complete graphs at d=0.99 -- checking every proven invariant after every step. Seeded PRNG makes every failure reproducible.

## Running

```bash
# Full test suite
cargo test

# Just the adversarial gauntlet
cargo test -p gauntlet

# Clippy lint sweep
cargo clippy --workspace --all-targets
```

## Status

These crates are the verified mathematical engine. To build a full Co-Wiki from them, you still need:

- A wiki backend (DokuWiki flat files, or similar) with filesystem integration
- Initial activation from query metadata (the `a^0` ignition function)
- Content similarity oracle for the dream operator
- A UI for human co-authorship, editing, and approval workflows
- Chat session harvest: extracting wiki-worthy content from LLM conversations

The primitives are ready. The integration is the next step.

## Citation

This work implements and formally verifies the architecture described in:

> Shomo, P. (2026). *The Co-Wiki and REM Agent: A Legible Memory Architecture for the Second Brain.* Licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

Per the author's request: this project cites the original design document and adopts the coined terminology (Co-Wiki, REM Agent, Legible Memory Stack).

## License

[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) -- same as the original Co-Wiki design document.
