# Ingestion (cl-ingest)

The `cl-ingest` workspace crate contains binaries for turning
CourtListener bulk data into a cowiki-ready corpus.

## Binaries

| binary | stage | purpose |
|---|---|---|
| `extract_opinion_cluster` | C | Stream `opinions.csv.bz2`, emit `(opinion_id, cluster_id)` CSV for a SCOTUS cluster filter. |
| `aggregate_citations` | D | mmap + rayon parallel aggregate of `citation-map.csv` → cluster-level edges. 0.7 s for the 77M-row citation map. |
| `extract_opinion_bodies` | — | Single-pass dump of (id, cluster_id, type, html_with_citations) as gzipped JSONL for SCOTUS opinions. |
| `enrich_scotus` | combined | One-shot enricher: `lbzip2 -dc` pipe → CSV parse → rayon HTML→markdown → parallel `.md` write. 18 minutes end-to-end. |
| `peek_opinion` | diag | Print the first N non-empty opinion rows. Useful when validating a new bulk-data schema. |

## Typical pipeline

```sh
# 1. Stage A (dockets) — Python, filter by court_id
python3 tools/ingest_courtlistener.py --court scotus --stage dockets

# 2. Stage B (opinion-clusters) — Python, filter by SCOTUS docket set
python3 tools/ingest_courtlistener.py --court scotus --stage clusters

# 3. Stage C (opinion→cluster map) — Rust
./target/release/extract_opinion_cluster \
    wiki-corpus/courtlistener-raw/opinions.csv.bz2 \
    wiki-corpus/courtlistener-raw/.cache/opinion_to_cluster.csv \
    --filter wiki-corpus/courtlistener-raw/.cache/scotus_cluster_ids.txt

# 4. Stage D (citation aggregation) — Rust
./target/release/aggregate_citations \
    wiki-corpus/courtlistener-raw/.cache/citation-map.csv \
    wiki-corpus/courtlistener-raw/.cache/opinion_to_cluster.csv \
    wiki-corpus/courtlistener-raw/.cache/scotus_cluster_ids.txt \
    wiki-corpus/courtlistener-raw/.cache/scotus_edges.csv

# 5. Stage E (emit markdown) — Python stage OR Rust enrich_scotus
./target/release/enrich_scotus \
    --opinions wiki-corpus/courtlistener-raw/opinions.csv.bz2 \
    --scotus-clusters wiki-corpus/courtlistener-raw/.cache/scotus_cluster_ids.txt \
    --clusters-meta wiki-corpus/courtlistener-raw/.cache/clusters_meta_583840.json \
    --edges wiki-corpus/courtlistener-raw/.cache/edges_495297.csv.gz \
    --out-o2c wiki-corpus/courtlistener-raw/.cache/opinion_to_cluster_all.csv \
    --out-dir wiki-corpus/scotus
```

## Dependencies

`aggregate_citations` and `enrich_scotus` both assume `lbzip2` is
installed:

```sh
sudo apt install lbzip2
```

Without it, `enrich_scotus` fails on spawn. `aggregate_citations`
operates on an already-decompressed CSV (decompress once with
`bzip2 -dk` first).

## Narrative

For the full story of how the pipeline evolved — single-threaded
Python at 1,350 rows/s stalling on a 77M-row file, to
parallel-decode Rust at 495M rows/s — see [Case Study: Parallel
ingest](../case-study/parallel-ingest.md) and [Case Study:
Enrichment](../case-study/enrichment.md).

<!-- TODO: `cl-ingest` binary flag reference for all five tools,
     input schema expectations, output format specs. -->
