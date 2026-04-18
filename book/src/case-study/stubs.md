# Building the stub corpus

*Skeleton chapter.*

The first SCOTUS corpus we produced had 495,297 pages averaging three
lines each. It was loadable, queriable, served reasonable latency —
and the search results were useless. This chapter tells that story
and extracts the lessons.

## Planned sections

- **The first Python pipeline** — stages A–E, what each produced.
- **The moment it stalled** — stage D, 77 million rows, one core,
  one very long night.
- **The first load** — 495,297 pages, density \\(9.6 \times 10^{-7}\\),
  94% of pages with no outgoing edges.
- **Search quality, honestly measured** — "commerce clause" returns
  Brown v. Department of Commerce. "Equal protection" returns EPA
  cases. What the algorithm was actually doing given its inputs.
- **The postmortem** — the algorithm was not wrong; the corpus was.
  The algorithm answered the question the data allowed.

<!-- TODO(next slice): write this chapter in full. -->
