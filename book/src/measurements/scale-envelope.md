# Scale envelope

*Skeleton chapter.*

What cowiki-rs can and cannot host, quantitatively. A ladder of
synthetic corpora at increasing sizes, with a dense-graph fixture
and a sparse-graph fixture at each rung, run in subprocess isolation
so per-rung RSS numbers are clean.

## Planned sections

- **The ladder** — ba-1000-4, ba-2500-6, ba-5000-6, ba-10000-8,
  ba-25000-8, clique-200, clique-500. What each models.
- **How to read the `scale_probe` output** — one JSON line per
  rung, with build_idx_ms, q_p50_us, q_p99_us, rebuild_ms,
  update_ms, save_ms, load_ms, rss_mb, converged_pct.
- **Per-rung RSS isolation** — why the cumulative in-process
  ladder inflates later rungs (glibc doesn't return freed pages),
  why we moved to subprocess probes in commit `f34b9f8`.
- **Extrapolation bounds** — we've measured up to 500k; beyond
  that, the next bottleneck is likely the activation vector size
  (\\(n\\) f64 entries = 4 MiB per in-flight query at 1 M nodes).
  Part III, Chapter 2 of DDIA has the mental model for what
  comes next.

<!-- TODO(next slice): write this chapter in full. -->
