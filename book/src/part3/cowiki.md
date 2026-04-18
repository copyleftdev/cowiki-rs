# cowiki

*This chapter is a skeleton. Filling in is scheduled for the next
writing slice.*

The `cowiki` crate is the composition layer — about 80 lines of code
that sequence `tfidf::ignite` → `spread` → `budget_knap::select`
into a retrieval pipeline, and `decay` → `prune` → `dream` into a
maintenance pipeline. It imports from every primitive in Part II
and provides nothing the primitives don't already provide.

## Planned sections

- **The `retrieve` function** — full source, line by line. The
  15-line heart of the system.
- **The `maintain` function** — how `rem_cycle` is wired into
  production.
- **Why this layer exists at all** — the discipline of
  composition-only.
- **What the glue crate may NOT do** — the rule that new logic
  must push down to a primitive or up to the backend.

<!-- TODO(next slice): write this chapter in full. -->
