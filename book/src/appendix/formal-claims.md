# Formal claims

*Skeleton appendix. Links into the repository's `PROOF.md`.*

A cross-referenced index of every formal property the engine
claims to satisfy, with pointers to where each is stated, proved,
and tested.

## The claims

- **Row-stochastic adjacency** — preserved by every public
  mutator of `ScoredGraph`. Stated in
  [Part II Chapter 1](../part2/scored-graph.md); tested in
  `crates/scored-graph/src/lib.rs::is_row_stochastic` + the
  gauntlet suite.
- **Contraction and convergence** — the spread iteration has
  Lipschitz constant \\(d \cdot L < 1\\). Proved in
  [Part II Chapter 2](../part2/spread.md); tested by
  `contraction_property` proptest.
- **Geometric envelope** — \\(r_t \le (d \cdot L)^t \cdot r_0\\).
  Corollary of the contraction proof; tested by the runtime
  audit's envelope assertion.
- **≥½-OPT knapsack** — modified-greedy selection is within a
  factor of 2 of the DP optimum. Proved in
  [Part II Chapter 3](../part2/budget-knap.md); tested in the
  runtime audit against `optimal_bruteforce` on 60 queries
  spanning 3 budgets.
- **Save/reload round-trip fidelity** — bit-identical raw
  weights and exact match on `TemporalState` after SQLite +
  sidecar round-trip. Tested by `save_reload_roundtrip` in the
  runtime audit.
- **Temporal invariants under REM cycles** — graph remains
  row-stochastic, dream edges respect all `ScoredGraph`
  invariants, total edge count non-decreasing. Tested by
  `vopr_long_horizon` over 500 cycles.

## Where to find the proofs

- `PROOF.md` in the repository root — the formal statement,
  with LaTeX. This book excerpts and narrates; `PROOF.md` is the
  authoritative form.
- The proptests in each primitive crate — executable form.
- The `runtime_audit` suite — the fixture matrix that asserts
  every claim on concrete corpora.

<!-- TODO(next slice): tie each claim to a specific commit hash
     and line number, so a reader can click through to the proof
     in the exact revision the book documents. -->
