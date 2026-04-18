# Introduction

`cowiki-rs` is a retrieval engine for associative search over an
authored document graph. A query ignites an initial activation vector
via TF-IDF, activation spreads along graph edges under a contraction
iteration, and a 0/1 knapsack selects the most valuable reachable
documents under a caller-supplied token budget. The engine is Rust,
the graph is row-stochastic, the iteration is provably convergent,
the knapsack is ≥½-OPT.

The production instance documented here serves the SCOTUS Explorer at
<https://scotus.cowiki.tech>: ten thousand of the most-cited Supreme
Court opinions with inline citation links, searchable via spreading
activation over a citation graph of 130k+ edges.

## What this documentation covers

- **Getting Started.** Install the binary, run the two shipped demos
  (`make demo`, `make explorer`), run the engine against a corpus of
  your own.
- **Overview.** Ten-minute read: what the engine is, what it's for,
  the layered-crate architecture.
- **Primitives and Glue.** Per-crate API reference — types, public
  functions, invariants, short examples.
- **HTTP API.** Endpoint reference for `cowiki-server`.
- **Operations.** Persistence layout, ingestion tooling, deployment
  patterns, observability surfaces.
- **Case Study: SCOTUS Explorer.** The one narrative section —
  what we built, what broke, what we changed.
- **Measurements.** Tables with experimental setup.
- **Appendix.** Formal claims cross-referenced to tests and
  `PROOF.md`; a glossary.

## Conventions

- **Code blocks** use Rust syntax for library references and shell
  syntax for commands. API signatures quote the `pub` items as of
  the current `main`.
- **File paths** are relative to the repository root.
- **`.claim`, `.postmortem`, `.aside` callouts** mark formal
  properties, things that went wrong, and digressions respectively.
- **Measurements** are given with their experimental setup. A number
  without a setup is a citation target, not a measurement.

## Repository layout

```text
cowiki-rs/
├── crates/
│   ├── scored-graph/       (primitive)
│   ├── spread/             (primitive)
│   ├── budget-knap/        (primitive)
│   ├── temporal-graph/     (primitive)
│   ├── chunk-quality/      (primitive)
│   ├── cowiki/             (glue — composition of primitives)
│   ├── wiki-backend/       (glue — filesystem + persistence)
│   ├── cowiki-server/      (glue — HTTP server)
│   ├── cl-ingest/          (CourtListener ingestion tools)
│   ├── gauntlet/           (adversarial test suite)
│   └── seed-corpus/        (synthetic fixtures for benchmarks)
├── ui/                     (operator dashboard)
├── ui-scotus/              (SCOTUS Explorer — product UI)
├── demo-wiki/              (20-page demo corpus)
├── wiki-corpus/            (production corpora; gitignored)
├── book/                   (this documentation)
├── proof/                  (Python property-test suite)
└── PROOF.md                (formal statement of invariants)
```

## Status

This documentation is maintained alongside the code on `main`. Every
push that touches `book/` rebuilds and redeploys the site. Chapters
that are not yet fleshed out are marked with a banner; their section
outlines are intentional so the navigation reflects intent.

The engine is in production at `scotus.cowiki.tech`. Tests pass at
the tag this documentation references (`main`). Measurements in the
Measurements chapter are reproducible from the repo using the
commands documented inline.
