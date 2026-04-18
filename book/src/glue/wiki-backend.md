# wiki-backend

Corpus I/O layer: filesystem scan, `[[link]]` parse, TF-IDF
index, graph construction, SQLite + CSR sidecar persistence.

*Reference skeleton — expanded content planned for the next
documentation slice.*

## Public API (selected)

```rust
use wiki_backend::{WikiBackend, types::{PageId, PageMeta}};
```

| signature | purpose |
|---|---|
| `WikiBackend::open(root: &Path) -> Result<Self, WikiError>` | scan + build from markdown |
| `WikiBackend::open_or_rebuild(root: &Path) -> Result<Self, WikiError>` | load persisted state if present, else scan |
| `fn save(&mut self) -> Result<(), WikiError>` | write SQLite + sidecars |
| `fn all_pages(&self) -> &[PageMeta]` | full metadata list |
| `fn page(&self, id: &PageId) -> Option<&PageMeta>` | O(1) lookup |
| `fn page_index(&self, id: &PageId) -> Option<usize>` | O(1) graph-index lookup |
| `fn graph(&self) -> &ScoredGraph` | borrow the graph |
| `fn ignite(&self, query: &str) -> Vec<f64>` | TF-IDF activation vector |
| `fn create_page(&mut self, id, title, content) -> Result<(), WikiError>` | incremental insert |
| `fn update_page(&mut self, id, content) -> Result<(), WikiError>` | incremental edit |
| `fn maintain_with_dream(&mut self, config: &RemConfig) -> HealthReport` | run REM cycle + persist |
| `fn len(&self) -> usize` | page count |
| `fn root(&self) -> &Path` | corpus root directory |

## Data flow

```text
  *.md + [[links]]  ──scan──▶  PageMeta[]  ──build──▶  ScoredGraph
                                 TfIdfIndex             + persist
```

## Invariants

- `PageMeta.path` is always relative to the corpus root.
- `PageId` is the relative path without the `.md` extension.
- `rebuild()` is called after every mutation (internal).
- Save/reload round-trip is bit-identical for raw weights;
  enforced by `save_reload_roundtrip` in the runtime audit.

## Persistence layout

See [Persistence (.cowiki/)](../ops/persistence.md) for the full
on-disk format.

<!-- TODO: full reference for types (PageMeta, WikiIndex), the
     scan + parse pipeline, TF-IDF inverted index internals,
     incremental create/update paths. -->
