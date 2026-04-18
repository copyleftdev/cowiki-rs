# Run the demos

Two shipped demos.

## `make demo` — the knowledge-graph dashboard

A 20-page corpus (`demo-wiki/`) covering AI, distributed systems,
cognition, and security. Dense interlinks (~92 edges), designed to
exercise every feature of the engine in a single corpus.

```sh
make demo
# → http://localhost:3001
```

UI surfaces:

- **Search tab.** Query a topic, see ranked results with snippets.
  Drill into a result to open the case drawer (neighborhood graph
  + article body).
- **Simulation tab.** Streams SSE telemetry while the server runs
  a synthetic workload (queries, creates, maintenance cycles).
- **Stress panel.** Runs N concurrent queries against the engine
  and reports p50/p95/p99 latency.
- **Perf panel.** Live-polled atomic counters from the server's
  hot path.
- **Corpus selector.** If multiple corpora are loaded, switch
  between them.

Stop with `make demo-stop`.

## `make explorer` — the SCOTUS Explorer

A product UI over the ten thousand most-cited Supreme Court
opinions, enriched with inline citation links. Runs on port 3002
alongside the demo.

```sh
make explorer
# → http://localhost:3002
make explorer-stop
```

UI surfaces:

- **Landing page.** Ten landmark cards (Brown, Miranda, Roe,
  Gideon, Wickard, Gibbons, McCulloch, Plessy, Marsh, Lochner)
  that open directly into the case drawer.
- **Doctrine-seeded search hints.** Nine constitutional doctrines
  pre-populated so a reader unfamiliar with the corpus has
  starting points.
- **Case drawer.** Opinion text with in-flow wiki-links that
  navigate between cases; radial neighborhood graph of the
  citation network around each case.

No diagnostic panels. No corpus switching. No operator controls.

The explorer requires the `wiki-corpus/scotus-top10k/` corpus,
which is not in the repository (700 MB of markdown). Reproduce
it from CourtListener bulk data using the tools described under
[Ingestion](../ops/ingestion.md) and [Case Study:
Enrichment](../case-study/enrichment.md).

## Direct invocation

Both demos ultimately run the `cowiki-server` binary. You can call
it directly for custom configurations:

```sh
./target/release/cowiki-server \
  <wiki-dir> [<wiki-dir> ...] \
  [--ui <dist-dir>] \
  [--port <N>] \
  [--read-only]
```

- Each positional argument is a corpus root; its directory
  basename becomes the corpus name in the API.
- `--ui <dist-dir>` serves a Vite build alongside the API.
- `--port <N>` (default 3001) also honors `COWIKI_PORT=N`.
- `--read-only` also honors `COWIKI_READ_ONLY=1`. Returns 403 on
  the three mutating endpoints.

Example — serve two corpora with the demo UI:

```sh
./target/release/cowiki-server \
  demo-wiki wiki-corpus/game-theory \
  --ui ui/dist --port 3001
```

The UI's corpus selector will show both; queries hit whichever is
active.
