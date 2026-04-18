# The product UI

*Skeleton chapter.*

The demo UI at `ui/` is a dashboard for operators — simulations,
stress tests, perf counters, corpus switchers. The SCOTUS Explorer
UI at `ui-scotus/` is a product for readers. This chapter documents
the deliberate subtraction that turned one into the other.

## Planned sections

- **What was removed** — simulation tab, stress controls, maintain
  button, perf panels, corpus selector, the all-pages sidebar.
- **What was added** — landmark cards on the landing page, doctrine-
  seeded search hints, case-drawer meta block, wiki-link click
  navigation between cases.
- **Design principles shared with the demo UI** — same type system,
  same color palette, same drawer component, same neighborhood
  graph. Product distinguishes itself through omission, not
  through new visual language.
- **The locked corpus pin** — `selectCorpus('scotus-top10k')` on
  mount. Harmless no-op if the server hosts one corpus; necessary
  if it hosts many.
- **The theme toggle and its localStorage key** — distinct from the
  demo's key, so users can have both open with different themes.

<!-- TODO(next slice): write this chapter in full. -->
