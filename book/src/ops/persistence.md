# Persistence (`.cowiki/`)

Persistent state for a corpus lives in a `.cowiki/` directory
inside the corpus root. Created on first `save()`.

## Layout

```text
<wiki-root>/
├── *.md                      authored pages
├── *.meta                    optional JSON sidecars per page
└── .cowiki/
    ├── engine.db             SQLite: pages JSON, tfidf postings,
    │                         temporal state, id_to_idx, costs
    ├── graph.row_ptr         u64 LE, (n+1) entries
    ├── graph.col_idx         u64 LE, nnz entries
    └── graph.values          f32 LE, nnz entries
```

## Why the CSR lives in sidecar files

The original schema stored raw weights as a BLOB in SQLite. At
\\(n \approx 11{,}500\\) the BLOB crossed the 1 GiB SQLite BLOB
size limit and writes panicked. Commit `c45aa48` moved weights to
three fixed-width sidecar files. Benefits:

- **No size cap** — files can grow past SQLite limits.
- **mmap-ready** — the naive fixed-width layout lets future
  loaders `bytemuck::cast_slice` over an mmap without a parser.
  Current loader uses `std::fs::read` + cast; mmap upgrade is a
  one-line change deferred until n > 1M.
- **Save becomes linear in `nnz`** instead of quadratic in
  \\(n\\). At \\(n = 25{,}000\\) save dropped from "impossible"
  to 1 second.

## What lives where

| in SQLite | in sidecars |
|---|---|
| pages JSON (titles, links_to, token_cost, category) | CSR row_ptr, col_idx, values |
| TF-IDF postings map | — |
| temporal state (activations, last_touched, alive, clock) | — |
| id_to_idx HashMap | — |
| costs (u64 per node) | — |
| graph metadata (n) | — |

## Save / reload fidelity

<div class="claim">

**Round-trip bit-identical.** For every corpus and every
maintenance sequence, `save()` then `open_or_rebuild()` produces
a graph whose `raw_weights`, `costs`, and
`TemporalState` exactly equal the pre-save state. Enforced by
`save_reload_roundtrip` in the runtime audit.

</div>

## Recovery

If `engine.db` is missing but sidecar files are present (or
vice versa), `open_or_rebuild()` falls back to a full scan from
markdown. The first save after recovery re-populates the missing
files.

Deleting `.cowiki/` is safe. The next server start rebuilds from
markdown (cold path).

<!-- TODO: full SQLite schema reference, migration story for
     pre-c45aa48 dense-blob databases, recovery procedures for
     partial corruption. -->
