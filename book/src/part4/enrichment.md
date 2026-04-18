# Enrichment

*Skeleton chapter.*

How we turned three-line stubs into articles with in-flow wiki-links
to every cited case, using the opinions' own text as the source.

## Planned sections

- **The `html_with_citations` discovery** — `plain_text` is often
  empty, but `html_with_citations` is populated and already has
  every citation resolved to an anchor tag with the target opinion
  id. Someone at CourtListener did the hard parsing work; we just
  had to recognize it.
- **Regex HTML → markdown** — four regex stages: resolve anchors to
  wiki-links, strip block tags to newlines, strip everything else,
  decode entities. Why this isn't BeautifulSoup.
- **Global opinion→cluster resolution** — an anchor points to an
  opinion_id, but our pages are clusters. The map between them is
  ~9 million entries; loading it into a DashMap and querying it
  from rayon workers was straightforward.
- **The result** — pages grew from 3 lines to ~200 lines;
  density climbed from \\(10^{-7}\\) to \\(10^{-3}\\); queries started
  finding landmark cases through graph traversal rather than title
  matching.

<!-- TODO(next slice): write this chapter in full. -->
