# Read endpoints

All read endpoints take a read lock and return without mutating
state. Unaffected by `--read-only`.

*Skeleton reference — full request/response schemas planned for
the next documentation slice.*

## `POST /api/query`

Retrieve under spreading activation.

| field | type | default |
|---|---|---|
| `query` | string | required |
| `budget` | u64 | 4000 |

Returns `{ pages: [{ id, title, token_cost, links_to: [id] }, …],
total_score, total_cost, converged, iterations, elapsed_us }`.

## `GET /api/pages`

Corpus page list.

| query param | type | default |
|---|---|---|
| `limit` | usize | unbounded |
| `order` | `"id"` \| `"hubs"` | `"id"` |

`order=hubs` returns top-N by outbound link count; O(N log k) via
heap selection. Used by the UI's hub-pill hints at 10k+ corpus
sizes.

## `GET /api/pages/{*id}`

Page detail including body content. ID is the full wiki path
(slashes allowed in the `*id` wildcard).

Returns `{ id, title, content, links_to: [id], token_cost }`.

Status codes:
- `200` — found
- `404` — no such page
- `502` — page indexed but body file missing on disk (filesystem
  diverged from index)

## `GET /api/neighborhood/{*id}`

Two-hop citation neighborhood around a center page.

Returns `{ center, nodes: [{id, title, token_cost, hops,
direction}], edges: [{from, to, weight}], truncated }`.

`direction` ∈ `{"center", "out", "in", "both", "indirect"}`. Caps
at 48 nodes; `truncated` indicates nodes were dropped.

Performance: O(Σ deg of frontier), not O(n × frontier). See
`crates/cowiki-server/src/main.rs::neighborhood_handler` for the
BFS walking `g.neighbors_out` + `g.neighbors_in` directly.

## `GET /api/stats`

`{ page_count, edge_count, density }` for the active corpus.

## `GET /api/perf`

Live atomic counters. `{ queries, query_avg_us, query_min_us,
query_max_us, maintains, maintain_avg_us, creates,
lock_acquisitions, lock_avg_ns }`. Reset on process restart.

## `POST /api/stress`

Run N concurrent queries against a fixed query set, return
latency distribution.

Request: `{ queries: [string], n: usize }`

Response: `{ n, total_us, avg_us, min_us, max_us, p50_us, p95_us,
p99_us, throughput_qps }`.

Percentiles use nearest-rank method — see `percentile()` in
`crates/cowiki-server/src/main.rs`.

## `GET /api/corpora`

List loaded corpora with their page/edge counts and which is
currently active.

## `GET /api/simulate?pages=N&ops=M`

SSE stream of simulation events. Creates a temporary corpus of N
pages, runs M mixed operations (query, create, maintain),
streams one event per operation.

Used by the demo UI's simulation tab.

<!-- TODO: full request/response JSON schemas, error codes,
     rate limits (none currently), idempotency notes. -->
