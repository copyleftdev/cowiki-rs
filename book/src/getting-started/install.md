# Install

## From source

Requires Rust 1.85+ (2024 edition) and Node 22+ for the UI bundle.

```sh
git clone https://github.com/copyleftdev/cowiki-rs
cd cowiki-rs
cargo build --release -p cowiki-server
cd ui && npm ci && npx vite build && cd ..
```

Outputs:

- `target/release/cowiki-server` — the HTTP server binary.
- `ui/dist/` — the demo UI bundle.

## With Docker

The repository includes a `deploy/Dockerfile` that builds the server
and demo UI and bundles the `demo-wiki` corpus into a single image.
`make demo` uses it end-to-end.

```sh
make demo        # builds, runs on http://localhost:3001
make demo-stop   # stops the container
```

For the SCOTUS Explorer UI and a standalone server instance:

```sh
make explorer        # http://localhost:3002, no corpus switching
make explorer-stop
```

Both Makefile targets are idempotent: re-running `make demo`
rebuilds only what changed.

## Verifying

With the server running:

```sh
curl -s http://localhost:3001/api/corpora | jq
# → [{"name":"demo-wiki","page_count":20,"edge_count":92,...}]

curl -s -X POST http://localhost:3001/api/query \
  -H 'content-type: application/json' \
  -d '{"query":"spreading activation","budget":4000}' | jq '.pages[0]'
```

Both return JSON. If either returns an error, see
[Observability](../ops/observability.md) for where the server logs.

## Running the test suite

```sh
cargo test                         # all 133 workspace tests
cargo test -p scored-graph         # one crate
cargo test -p wiki-backend --test vopr   # end-to-end simulation

cd proof && pip install -r requirements.txt && python -m pytest
# → 37 property tests against the formal claims in PROOF.md
```

Tests run without network or special permissions. A pass is the
contract for any contribution to the core crates.
