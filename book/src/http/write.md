# Write endpoints

All write endpoints acquire a write lock and return 403 when
the server was started with `--read-only` (or
`COWIKI_READ_ONLY=1`).

*Skeleton reference — full request/response schemas planned for
the next documentation slice.*

## `POST /api/pages`

Create a new page.

Request: `{ id: string, title: string, content: string }`

Returns `201 Created` on success. Incremental — calls
`wiki.create_page()` which updates the TF-IDF index and graph
without a full rebuild.

Persist failures return `500` with the error logged to stderr
(see F3 audit fix).

## `POST /api/maintain`

Run the REM cycle (decay + prune + dream) and persist.

Request body: `{}` (no parameters; REM config is server-wide).

Response: `{ health: f64, pruned_count: usize, dreamed_count:
usize, dreamed_edges: [[from_id, to_id]], elapsed_us: u64 }`.

Typical runtime: ~15 s on a 10k corpus with full text, ~17 s
on 495k stub corpus. Persists on completion; save failures
return `500` (F3 audit fix).

## `POST /api/corpora/select`

Set the active corpus.

Request: `{ name: string }`

Response: `204 No Content` on success, `404` if the named
corpus isn't loaded.

In `--read-only` mode this is also 403 — even though it
technically doesn't mutate persistent state. Rationale: in
production a hostile caller could flip the active corpus in a
way that confuses a concurrent reader; simpler to block.

## Read-only mode

Enable with `--read-only` or `COWIKI_READ_ONLY=1`:

```sh
./cowiki-server my-corpus --port 3002 --read-only
# API ready at http://0.0.0.0:3002  (default corpus: my-corpus) [read-only]
```

The `[read-only]` banner appears in stderr at boot, confirming
the mode. All three write endpoints return `403 Forbidden`; read
endpoints are unaffected.

Used for production deploys where the server is the only guard
between the public internet and the corpus. No edge proxy
required.

<!-- TODO: full request/response JSON schemas, error codes,
     auth model (currently none — the assumption is a private
     deployment or reverse-proxy-level auth). -->
