# End-to-end

*Skeleton chapter.*

Every number the book has claimed, stated with its experimental
setup. One table per axis, one caveat per table.

## Planned tables

- **Ingest timing** — 100-min projected → 18-min actual, stage by
  stage.
- **Query latency** — p50, p95, p99 for each corpus size tier
  (20 / 1.4k / 10k / 495k).
- **Cold vs warm boot** — 18s cold (markdown rescan) vs 3s warm
  (`.cowiki` sidecar).
- **RSS** — at rest, during query burst, during maintain cycle.
- **Neighborhood request** — before and after the O(n·frontier) →
  O(Σ deg) fix.
- **`/api/pages` payload** — before and after `?limit=`.
- **Image size** — with and without the chown-layer duplicate.

## Planned caveats

- Box specs: 64 cores, 247 GiB RAM. These numbers do not
  extrapolate linearly.
- Production numbers are from App Platform `apps-d-2vcpu-8gb`; the
  query latency is ~3× what the dev box reports because of the core
  count difference.
- First-visit numbers include TLS handshake; subsequent numbers
  don't.

<!-- TODO(next slice): write this chapter in full. -->
