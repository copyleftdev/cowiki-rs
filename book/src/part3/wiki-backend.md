# wiki-backend

*Skeleton chapter.*

`wiki-backend` is the layer between "a directory of markdown files
with `[[backlinks]]`" and "a working `ScoredGraph` with TF-IDF index
and persistence." It owns the scan, the parse, the graph
construction, the TF-IDF index, and the `.cowiki/` persistence format.

## Planned sections

- **Scan and parse** — walking a directory, extracting
  `[[target|display]]` links, producing `PageMeta[]`.
- **TF-IDF construction** — the inverted-index postings layout, what
  changed from the dense-vector original, RSS savings.
- **The `WikiIndex` type** — pages, `id_to_idx`, `raw_weights`.
- **Graph construction** — how `PageMeta.links_to` becomes edges.
- **Persistence** — SQLite + sidecars, save/load symmetry.
- **Incremental `create_page` and `update_page`** — how write
  latency went from seconds to milliseconds.
- **The `page_index` accessor** — O(1) lookup we added for the
  neighborhood endpoint.

<!-- TODO(next slice): write this chapter in full. -->
