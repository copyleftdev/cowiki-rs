# cowiki-server

HTTP server binary. Axum + `parking_lot::RwLock` shared state;
serves API, SSR routes, and the static UI bundle.

*Reference skeleton — expanded content planned for the next
documentation slice.*

## Binary

```sh
cowiki-server <wiki-dir> [<wiki-dir> ...]
              [--ui <dist-dir>]
              [--port <N>]        # also honors COWIKI_PORT
              [--read-only]       # also honors COWIKI_READ_ONLY=1
```

Each positional argument is a corpus root; its basename becomes
the corpus name in `/api/corpora`. Default active corpus is the
first one alphabetically (BTreeMap order).

## Shared state

```rust
struct Inner {
    corpora: BTreeMap<String, RwLock<WikiBackend>>,
    active: RwLock<String>,
    counters: Counters,    // atomic perf counters
    read_only: bool,
}
```

**Read endpoints** take `acquire_wiki()` → `RwLockReadGuard`.
Multiple readers proceed concurrently.

**Write endpoints** take `acquire_wiki_mut()` → `RwLockWriteGuard`.
Excluded by any active reader.

## Routes

- **[Read endpoints](../http/read.md)** — query, pages list, page
  detail, neighborhood, stats, perf, stress, corpora, simulate
  (SSE).
- **[Write endpoints](../http/write.md)** — create_page, maintain,
  corpora/select. All return 403 when `--read-only` is active.
- **[SSR routes](../http/ssr.md)** — `/w/{corpus}/{*id}`,
  `/c/{corpus}`, sitemap, robots.

## Middleware

- `CorsLayer::permissive()` — CORS on everything.
- Cache-control middleware on static `/assets/*` — see
  [Deployment](../ops/deployment.md) for the rationale (DO App
  Platform overrides with `private` by default).

## Observability

Per-request `RwLock` wait time is accumulated atomically in
`counters.lock_wait_ns_total`. See
[Observability](../ops/observability.md) for `/api/perf`
semantics.

<!-- TODO: full routing table with handler signatures, Inner
     construction, SSR content-type/status matrix, simulate
     event schema. -->
