# cowiki-server

*Skeleton chapter.*

`cowiki-server` is the Axum process that exposes the engine over
HTTP, serves the UI bundle, and hosts the SSR surface for crawlers.

## Planned sections

- **The `Inner` state** — corpora as `BTreeMap<String,
  RwLock<WikiBackend>>`, why RwLock over Mutex, F4 audit history.
- **Read endpoints** — query, get_page, neighborhood, stats, perf,
  stress, corpora, pages list w/ pagination.
- **Write endpoints** — create_page, maintain, corpora_select, and
  the `--read-only` guard that blocks them on public deploys.
- **SSR routes** — `/w/{corpus}/{*id}`, `/c/{corpus}`, sitemap,
  robots. Why SSR matters for sharing and indexing.
- **The simulate SSE stream** — telemetry for the demo UI's
  simulation tab.
- **Static asset serving** — cache-control middleware, DO's
  `private` default, why we override.
- **CLI flags and env vars** — `--ui`, `--port`, `--read-only`,
  `COWIKI_*` equivalents.

<!-- TODO(next slice): write this chapter in full. -->
