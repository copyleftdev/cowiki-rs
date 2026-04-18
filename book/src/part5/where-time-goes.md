# Where the time goes

*Skeleton chapter.*

A profiler's-eye view of a single query. What fraction of
wall-clock time is in TF-IDF ignition, in the spread iteration, in
the knapsack, in serialization, in the lock acquisition.

## Planned sections

- **The `/api/perf` counters** — what they measure, how they're
  incremented atomically on the hot path, what they miss.
- **`strace -c -f` under burst load** — a breakdown of syscall
  time. Historical note: before the RwLock migration, 94% of
  server CPU time was in `futex` (lock contention); after, it's
  dominated by `recvfrom` and `sendto` (actual work).
- **Flamegraph from `profile_harness`** — where the budget is
  within `spread::spread` and `ignite`. Spoiler: `adj` binary
  search and `atoi` account for most of it; not obvious before
  measurement.
- **Budget for a 1-million-node corpus** — ~30 ms per query is the
  ceiling the current implementation can sustain on a single
  CPU. What you'd have to change to push past it.

<!-- TODO(next slice): write this chapter in full. -->
