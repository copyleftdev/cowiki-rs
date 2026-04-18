# Your own corpus

A corpus is a directory of Markdown files with `[[wiki-style]]`
internal links. cowiki-rs scans the directory on startup, parses
the links into a graph, builds a TF-IDF index, and is ready to
serve queries.

## Minimum corpus

```text
my-corpus/
├── index.md
├── foo.md
└── subdir/
    └── bar.md
```

Every `.md` file becomes a page. Its **page ID** is the relative
path from the corpus root, without the `.md` extension:

```
index.md       →  index
foo.md         →  foo
subdir/bar.md  →  subdir/bar
```

## Links

Inside any `.md` file, reference other pages with `[[target]]` or
`[[target|display text]]`:

```markdown
See also [[subdir/bar]] and [[foo|the foo page]].
```

Link resolution uses the page ID. Dangling links (targets that
don't exist in the corpus) are dropped from the graph silently —
this is deliberate, so references to not-yet-created pages don't
block indexing.

## Metadata

Optional `.meta` JSON sidecars next to each `.md` file override
indexing behavior:

```json
{
  "id": "subdir/bar",
  "title": "Page title shown in UI",
  "links": ["foo", "subdir/baz"],
  "token_cost": 142,
  "category": "optional-category-tag"
}
```

If a `.meta` file is absent, the server infers the same fields:
`title` from the first `# Heading` in the markdown, `links` from
parsed `[[link]]` occurrences, `token_cost` from a cheap tokenizer
over the body, `category` unset. The `.meta` file wins for fields
it contains.

## Starting the server

```sh
./target/release/cowiki-server my-corpus
# → API ready at http://0.0.0.0:3001  (default corpus: my-corpus)
```

No flags needed. First-boot scans every file, builds the graph and
TF-IDF index, then listens. For small corpora (< 1k pages) this is
under a second. For 500k pages on a commodity box, count on ~30s
cold; subsequent boots are ~3s after the first `/api/maintain`
saves the `.cowiki/` persistence sidecars.

## Example query

```sh
curl -s -X POST http://localhost:3001/api/query \
  -H 'content-type: application/json' \
  -d '{"query":"your search terms","budget":4000}' \
  | jq '.pages[] | {id, title, token_cost}'
```

`budget` is the total token allowance; cowiki-rs packs as much
relevant content as fits.

## What makes a corpus work well

- **Enough pages** that spreading has somewhere to spread to.
  Under ~50 pages and the iteration's value is marginal over pure
  TF-IDF.
- **Authored links.** If the pages don't cite each other, there's
  no graph signal; see [The typed-graph bet](../overview/what.md)
  for why this matters.
- **Substantive text.** Three-line stubs match poorly on TF-IDF.
  Real paragraphs of content are what the engine was built for.

See the [Case Study](../case-study/premise.md) for what happens
when one of these conditions fails (stub content on the first
SCOTUS corpus) and how it was fixed (enrichment from opinion
text).
