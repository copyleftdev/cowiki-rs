# cowiki-rs

Formally verified spreading activation engine for the [Co-Wiki and REM Agent](https://gist.github.com/paulshomo/69cf99e3185fa7ad0f50fc0e38bcd424) architecture proposed by [Paul Shomo](https://www.linkedin.com/in/paulshomo/) ([@ShomoBits](https://x.com/ShomoBits)).

## Try it

```bash
git clone https://github.com/copyleftdev/cowiki-rs
cd cowiki-rs
make demo
```

Browser opens. 20-page wiki, 92 backlink edges, spreading activation retrieval, performance counters, stress testing, REM agent with dream-discovered backlinks. All real, no mocks.

Query "memory sleep consolidation" and watch activation spread from memory-consolidation through backlinks into priming, REM agent, chunking, and Thinking Fast and Slow. 68 microseconds. Vector search finds one page. Spreading activation finds seven.

`make demo-stop` to shut down. Requires Docker.

## What this is

Shomo proposed a wiki-based warm storage layer for LLM memory where retrieval happens through **graph-based spreading activation** instead of flat vector search. He published the design as an open invitation to build.

We formalized the math, proved it works, found three corrections, built Rust crates from the verified contracts, wired them to a real wiki filesystem, added a performance dashboard, and Dockerized the whole thing.

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

Findings 8 and 9 are corrections that improved the model. The sigmoid threshold fix restores all guarantees. See [`PROOF.md`](PROOF.md) for the full verification report.

## Architecture

```
cowiki-rs/
  PROOF.md                Formal verification report (17 claims, 3 corrections)
  proof/                  Python hypothesis suite (37 property tests)
  demo-wiki/              20 interconnected pages across 5 topic clusters
  ui/                     React dashboard (Vite)
  Dockerfile + Makefile   One-command demo
  crates/
    scored-graph/         Weighted directed graph, row-stochastic invariant
    spread/               Spreading activation, pluggable thresholds
    budget-knap/          Budget-constrained selection, >= 1/2 OPT guarantee
    temporal-graph/       REM Agent: decay, prune, dream
    chunk-quality/        Coherence, recall, density metrics
    cowiki/               Composition layer
    gauntlet/             VOPR adversarial chaos suite (41 tests)
    wiki-backend/         Filesystem scan, TF-IDF, SQLite + .meta persistence
    cowiki-server/        Axum HTTP API + static UI serving
```

### How it works

```
wiki directory           wiki-backend              cowiki primitives
*.md files     --scan-->  PageMeta[]     --build-->  ScoredGraph
[[backlinks]]             TfIdfIndex                 spread(a0)
directories               id_to_idx                  select(budget)
               <--write-- create_page()  <--------   rem_cycle()
.cowiki/                   persist()                  dream_candidates()
  engine.db   (SQLite, computational state)
  *.meta      (human-readable, cat-able)
```

### The demo UI

Three-panel dashboard. Left: query with example pills + page list. Center: page viewer with clickable backlink pills. Right: live performance counters, stress test (fires 200 queries, shows p50/p95/p99 latency bars), REM agent controls with health ring gauge and dream-discovered backlinks.

Kinetic mutex indicator in the header: pulsing green when idle, glowing amber when locked. Lock-wait time measured in nanoseconds.

## Performance

Profiled with Valgrind callgrind (25B instructions), cachegrind, and massif. Three optimization passes driven by the profiling data.

| Metric | Value |
|---|---|
| Query latency (p50) | 27 us |
| Throughput | 36,400 qps |
| Save/reload | 12 ms |
| REM + dream cycle | 2.6 ms |

## Test suite

**133 tests, 0 failures, 0 clippy warnings, 0 unsafe.**

| Layer | Tests | What it covers |
|---|---|---|
| proptest (property-based) | 22 | Contraction, convergence, bounds, knapsack, decay, coherence |
| Unit tests | 61 | Construction, edge cases, scan, parse, TF-IDF, persistence, pipeline |
| Gauntlet (adversarial) | 41 | VOPR chaos, pathological topologies, IEEE 754 torture, worst-case knapsack |
| Backend VOPR (end-to-end) | 9 | Filesystem chaos: create/edit/query/maintain/save/reload/external deletion |

The gauntlet runs ~22,500 chaos operations checking every proven invariant after every step. The backend VOPR drives the full vertical (filesystem through SQLite through spreading activation) under random operations for 500+ steps per seed.

## Running without Docker

```bash
# Terminal 1: API server
cargo build --release -p cowiki-server
./target/release/cowiki-server demo-wiki

# Terminal 2: UI dev server (with hot reload)
cd ui && npm install && npx vite

# Or serve the built UI from the server itself:
cd ui && npm install && npx vite build
./target/release/cowiki-server demo-wiki --ui ui/dist
```

```bash
# Run all tests
cargo test

# Run the Python proof suite
cd proof && pip install -r requirements.txt && python -m pytest tests/ -v
```

## Demo wiki

20 pages across 5 topic clusters with organic cross-domain backlinks:

- **cognitive/** chunking, memory-consolidation, priming, spreading-activation
- **security/** attack-surface-mapping, threat-modeling, supply-chain, sbom-analysis
- **distributed/** eventual-consistency, consensus-protocols, fault-injection
- **ai/** spreading-activation (formal), knapsack-retrieval, transformers, attention
- **projects/** threat-model-review
- **reading-notes/** Designing Data-Intensive Applications, Thinking Fast and Slow

The cross-domain links are what make spreading activation shine. A query about "memory" reaches security pages through the shared concept of graph traversal. A query about "trust" reaches cognitive science through consensus protocols.

## Citation

This work implements and formally verifies the architecture described in:

> Shomo, P. (2026). *The Co-Wiki and REM Agent: A Legible Memory Architecture for the Second Brain.* Licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

Per the author's request: this project cites the original design document and adopts the coined terminology (Co-Wiki, REM Agent, Legible Memory Stack).

## License

[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)
