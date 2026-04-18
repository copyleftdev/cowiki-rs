# Curating the landmarks

*Skeleton chapter.*

How we went from 495k pages to 10k, and why that was the move that
made the product feel right.

## Planned sections

- **The top-N-by-citation-count rank** — what it actually contains
  at different thresholds. Top 10 is procedural workhorses (Ashcroft
  v. Iqbal, Twombly). Top 1,000 starts catching constitutional
  landmarks. Top 10,000 covers most famous cases plus a healthy
  shoulder.
- **The cert-denial contamination** — ~3.8% of top-10k were
  certiorari denials with one-line bodies. A user drill-in landed
  on one, saw an empty neighborhood graph, and reported "UI is
  broken." Lesson: *citation_count is not a quality signal for
  cowiki-rs specifically.*
- **The body-length filter fix** — require opinion body ≥ 2 KiB,
  keep top-N of what remains. 380 cert denials out; 380 longer
  cases in; edge count gained ~4k.
- **The resulting corpus** — 10,000 pages, 133,682 edges (including
  6,637 dreamed during warm-up), density \\(1.34 \times 10^{-3}\\).

<!-- TODO(next slice): write this chapter in full. -->
