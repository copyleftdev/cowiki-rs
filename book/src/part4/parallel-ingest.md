# Parallel ingest

*Skeleton chapter.*

The move from a single-threaded Python pipeline to a parallel Rust
pipeline. Twenty to sixty times faster, depending on which stage
you measure.

## Planned sections

- **The bottleneck, named** — `libbz2` is single-threaded. At
  ~30 MB/s decode on 54 GiB of opinions data, that's 30 minutes of
  pure decode before CPU gets to do anything else.
- **The `aggregate_citations` rewrite** — mmap + memchr + rayon +
  ahash. 495 million rows per second on 64 cores, 0.7 s end-to-end
  on the 2.57 GiB decompressed citation map.
- **The CSV-quoting gotcha** — CourtListener's PostgreSQL export
  uses backslash-escape for quotes. `atoi` stops at the first
  non-digit; we spent a surprised hour finding this.
- **The `enrich_scotus` binary** — `lbzip2 -dc` piped into a
  streaming CSV parser, parallel HTML→markdown in rayon, DashMap
  accumulation by cluster, parallel write. ~18 minutes end-to-end
  for 516,690 opinions.
- **Measurements** — the full timing table; what scales with cores,
  what scales with disk, what scales with compression ratio.

<!-- TODO(next slice): write this chapter in full. -->
