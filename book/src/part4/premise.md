# The premise and the data

*Skeleton chapter.*

Why SCOTUS. Why CourtListener. What their bulk-data dump contains,
what it doesn't, and what we thought we'd have versus what we actually
had when the first pipeline ran end-to-end.

## Planned sections

- **The thesis for a SCOTUS explorer** — legal citation graphs are
  the canonical dense-graph corpus; if cowiki-rs doesn't work on
  SCOTUS it won't work on much.
- **CourtListener's bulk-data layout** — dockets, opinion-clusters,
  opinions, citation-map. Sizes, schemas, license.
- **What we hoped for vs what we got** — landmark coverage turned
  out to be uneven; cert denials inflate the citation_count ranking;
  most opinions lack plain_text but have html_with_citations.

<!-- TODO(next slice): write this chapter in full. -->
