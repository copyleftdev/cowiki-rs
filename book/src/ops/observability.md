# Observability

cowiki-server exposes live per-process counters over HTTP and
logs to stderr. No external observability dependency; bring your
own scraper.

## `/api/perf`

Live atomic counters, reset on process restart.

```json
{
  "queries":            2001,
  "query_avg_us":       5449.8,
  "query_min_us":       4858,
  "query_max_us":       97392,
  "maintains":          0,
  "maintain_avg_us":    0.0,
  "creates":            0,
  "lock_acquisitions":  2003,
  "lock_avg_ns":        26.34
}
```

- `query_*` — latency of `/api/query` handler end-to-end.
- `maintain_*` — latency of `/api/maintain` (REM cycle + persist).
- `creates` — count of successful `POST /api/pages`.
- `lock_acquisitions` — count of RwLock `acquire_wiki()` +
  `acquire_wiki_mut()` calls.
- `lock_avg_ns` — mean wait time before the lock was granted.
  A high value (tens of µs or more) indicates contention.

## `/api/stats`

Per-corpus structural snapshot. Cheap; poll as often as you want.

```json
{ "page_count": 10000, "edge_count": 133682, "density": 0.00134 }
```

## `/api/corpora`

Enumerate loaded corpora and which is active. Useful for
health-checking a multi-corpus deployment.

## Startup banner

stderr at process start:

```text
Opening corpus 'scotus-top10k' at wiki-corpus/scotus-top10k
  indexed 10000 pages
Serving UI from: ui-scotus/dist
Co-Wiki ready at http://0.0.0.0:3002  (default corpus: scotus-top10k) [read-only]
```

The bracketed mode annotations (`[read-only]`) are useful in
log scraping to detect accidental deploys without the flag.

## Suggested thresholds

| metric | warn | page |
|---|---|---|
| process RSS | > 5 GB (10k corpus) | > 6 GB |
| `query_avg_us` | > 20,000 | > 100,000 |
| `lock_avg_ns` | > 10,000 | > 1,000,000 |
| HTTP 5xx / min | > 5 | > 20 |
| systemd restart rate | > 1 / hr | > 3 / hr |

## VOPR simulation as a staging check

Before a production deploy, run the VOPR long-horizon suite:

```sh
cargo test --release -p wiki-backend --test vopr -- --ignored
```

Runs 500 REM cycles over a synthetic corpus with random queries
interleaved. Catches invariant breaks that shorter tests miss
(see the f32 subnormal fix at step 409 in commit `fe65144`).

<!-- TODO: Prometheus exporter pattern, log-format spec,
     correlation id convention for distributed deploys. -->
