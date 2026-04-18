#!/usr/bin/env python3
"""
Phase 2 of the SCOTUS enrichment: take the extracted
`scotus_opinion_bodies.jsonl.gz` + the existing cluster metadata and the
global opinion→cluster map, and rewrite `wiki-corpus/scotus/*.md` so each
page has real article body text with in-flow wiki-links.

Input
-----
- wiki-corpus/courtlistener-raw/.cache/scotus_opinion_bodies.jsonl.gz
    one line per SCOTUS opinion: {"id":N,"cid":M,"type":"...","html":"..."}
- wiki-corpus/courtlistener-raw/.cache/opinion_to_cluster_all.csv
    every opinion_id → cluster_id (needed so we can resolve in-text
    citation anchors that point to non-SCOTUS cases as cleanly as to
    SCOTUS ones)
- wiki-corpus/courtlistener-raw/.cache/clusters_meta_583840.json
    SCOTUS cluster metadata (case_name, date_filed, etc)
- wiki-corpus/courtlistener-raw/.cache/edges_495297.csv.gz
    aggregated cluster→cluster edges for the existing "Cites" footer

Output
------
- wiki-corpus/scotus/{slug-cid}.md (rewritten)
"""

from __future__ import annotations
import gzip
import json
import multiprocessing as mp
import re
import sys
import time
from collections import defaultdict
from html import unescape
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "wiki-corpus" / "courtlistener-raw"
CACHE = RAW / ".cache"
OUT_DIR = ROOT / "wiki-corpus" / "scotus"


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# ─── slug (must match tools/ingest_courtlistener.py::slugify) ───────────────

def slugify(s: str, max_len: int = 80) -> str:
    out = []
    prev_dash = False
    for c in s.lower():
        if c.isalnum():
            out.append(c)
            prev_dash = False
        elif not prev_dash:
            out.append("-")
            prev_dash = True
    return "".join(out).strip("-")[:max_len] or "case"


def page_id(cluster_id: int, meta: dict) -> str:
    base = slugify(meta.get("case_name_short") or meta.get("case_name") or f"case-{cluster_id}")
    return f"{base}-{cluster_id}"


# ─── HTML → markdown ────────────────────────────────────────────────────────
#
# CourtListener's html_with_citations is reasonably regular: block elements
# (<p>, <center>, <blockquote>, <ol>, etc.), inline elements (<em>, <strong>,
# <span>), and citation anchors:
#
#   <a href="/opinion/N/slug/" aria-description="...">TEXT</a>
#
# We don't need a real HTML parser. Regex + html.unescape does what we need
# and is 100× faster than BeautifulSoup across 467k documents.

RX_CITATION = re.compile(
    r'<a\s+[^>]*?href="/opinion/(\d+)/[^"]*"[^>]*>([\s\S]*?)</a>',
    re.IGNORECASE,
)
RX_BLOCK_BREAKS = re.compile(
    r'</?(?:p|div|center|blockquote|ol|ul|li|br|hr|h[1-6]|pre|table|tr)\b[^>]*>',
    re.IGNORECASE,
)
RX_TAG_ANY = re.compile(r'<[^>]+>')
RX_MULTI_BLANKS = re.compile(r'\n{3,}')
RX_WHITESPACE_RUNS = re.compile(r'[ \t]+')


# Patched in by each worker via initializer
_O2C_ALL: dict[int, int] = {}
_CLUSTER_SLUG: dict[int, str] = {}


def _init_worker(o2c_all: dict, cluster_slug: dict) -> None:
    global _O2C_ALL, _CLUSTER_SLUG
    _O2C_ALL = o2c_all
    _CLUSTER_SLUG = cluster_slug


def _strip_inner(text: str) -> str:
    """Remove any nested tags from anchor inner text; keep the surface text only."""
    return RX_TAG_ANY.sub('', text)


def _replace_citation(m: re.Match) -> str:
    opinion_id = int(m.group(1))
    anchor_text = _strip_inner(m.group(2)).strip()
    cluster = _O2C_ALL.get(opinion_id)
    if cluster is not None:
        slug = _CLUSTER_SLUG.get(cluster)
        if slug:
            anchor = anchor_text.replace('|', ' ').replace('[', '(').replace(']', ')') or str(cluster)
            return f"[[{slug}|{anchor}]]"
    return anchor_text


def html_to_markdown(html: str) -> str:
    # Resolve citation anchors first — otherwise the tag-strip eats them.
    html = RX_CITATION.sub(_replace_citation, html)
    # Block-level tags become newlines so paragraphs survive.
    html = RX_BLOCK_BREAKS.sub('\n', html)
    # Everything else (spans, emphasis, anchors to non-opinion targets, etc.)
    # collapses to surface text.
    html = RX_TAG_ANY.sub('', html)
    html = unescape(html)
    html = RX_WHITESPACE_RUNS.sub(' ', html)
    html = RX_MULTI_BLANKS.sub('\n\n', html)
    return html.strip()


# ─── opinion-type → heading ─────────────────────────────────────────────────

TYPE_HEADINGS = {
    "010combined":   "Opinion",
    "015unamimous":  "Opinion",
    "020lead":       "Opinion",
    "025plurality":  "Plurality Opinion",
    "030concurrence":   "Concurrence",
    "035concurrenceinpart": "Concurrence in Part",
    "040dissent":    "Dissent",
    "050addendum":   "Addendum",
    "060remittitur": "Remittitur",
    "070rehearing":  "On Rehearing",
    "080onthemerits":"On the Merits",
    "090onmotiontostrike": "On Motion to Strike",
    "100trialcourt": "Trial Court Opinion",
}
TYPE_ORDER = {
    "010combined": 0, "015unamimous": 0, "020lead": 0, "025plurality": 1,
    "030concurrence": 2, "035concurrenceinpart": 2,
    "040dissent": 3,
    "050addendum": 4, "060remittitur": 5, "070rehearing": 6,
    "080onthemerits": 7, "090onmotiontostrike": 8, "100trialcourt": 9,
}


def type_heading(t: str) -> str:
    return TYPE_HEADINGS.get(t, t.replace('_', ' ').title() or 'Opinion')


def type_order(t: str) -> int:
    return TYPE_ORDER.get(t, 99)


# ─── worker: convert one cluster's opinions ─────────────────────────────────

def convert_cluster(args: tuple[int, list[dict]]) -> tuple[int, str]:
    """(cluster_id, [opinion_rows]) → markdown body (heading + sections)."""
    cluster_id, opinions = args
    opinions.sort(key=lambda o: (type_order(o.get('type', '')), o.get('id', 0)))

    sections: list[str] = []
    for opn in opinions:
        html = opn.get('html') or ''
        if not html:
            continue
        body = html_to_markdown(html)
        if not body:
            continue
        heading = type_heading(opn.get('type', ''))
        sections.append(f"## {heading}\n\n{body}\n")
    return cluster_id, '\n'.join(sections)


# ─── driver ─────────────────────────────────────────────────────────────────

def load_clusters_meta() -> dict[int, dict]:
    path = CACHE / "clusters_meta_583840.json"
    log(f"loading clusters meta ({path.stat().st_size // (1<<20)} MiB)")
    return {int(k): v for k, v in json.loads(path.read_text()).items()}


def load_global_o2c() -> dict[int, int]:
    path = CACHE / "opinion_to_cluster_all.csv"
    log(f"loading global opinion→cluster map ({path.stat().st_size // (1<<20)} MiB)")
    m: dict[int, int] = {}
    with path.open('r') as f:
        for line in f:
            try:
                a, b = line.rstrip().split(',')
                m[int(a)] = int(b)
            except ValueError:
                continue
    log(f"  {len(m):,} opinion→cluster entries")
    return m


def load_edges() -> list[tuple[int, int, int]]:
    path = CACHE / "edges_495297.csv.gz"
    log(f"loading aggregated cluster edges")
    out: list[tuple[int, int, int]] = []
    with gzip.open(path, 'rt') as f:
        for line in f:
            a, b, d = line.rstrip().split(',')
            out.append((int(a), int(b), int(d)))
    log(f"  {len(out):,} edges")
    return out


def stream_opinions_by_cluster(scotus_clusters: set[int]) -> dict[int, list[dict]]:
    path = CACHE / "scotus_opinion_bodies.jsonl.gz"
    log(f"loading SCOTUS opinion bodies from {path.name}")
    per_cluster: dict[int, list[dict]] = defaultdict(list)
    n = 0
    with gzip.open(path, 'rt') as f:
        for line in f:
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                continue
            cid = row.get('cid')
            if cid is None or cid not in scotus_clusters:
                continue
            per_cluster[cid].append(row)
            n += 1
            if n % 50_000 == 0:
                log(f"    … {n:>8} opinions indexed")
    log(f"  {n:,} opinions across {len(per_cluster):,} clusters")
    return per_cluster


def build_header(cluster_id: int, meta: dict) -> str:
    name = meta.get('case_name') or meta.get('case_name_short') or f"Case {cluster_id}"
    date = meta.get('date_filed', '')
    status = meta.get('precedential_status', '')
    incoming = meta.get('citation_count', 0)

    lines = [f"# {name}", ""]
    bits = []
    if date: bits.append(f"filed {date}")
    if status: bits.append(status.lower())
    if incoming: bits.append(f"{incoming} incoming citations")
    if bits:
        lines.append("*" + ", ".join(bits) + "*")
        lines.append("")
    return '\n'.join(lines)


def build_cites_footer(
    cluster_id: int,
    outgoing: dict[int, list[tuple[int, int]]],
    clusters: dict[int, dict],
    id_by_cluster: dict[int, str],
) -> str:
    cites = outgoing.get(cluster_id, [])
    if not cites:
        return ''
    cites = sorted(cites, key=lambda x: x[1], reverse=True)[:200]
    lines = [f"## Cites ({len(cites)})", ""]
    for target_cid, depth in cites:
        target_slug = id_by_cluster.get(target_cid)
        if not target_slug:
            continue
        target_name = (
            clusters.get(target_cid, {}).get('case_name_short')
            or clusters.get(target_cid, {}).get('case_name', '')
        ).strip()
        marker = f" ×{depth}" if depth > 1 else ""
        safe_name = target_name.replace('|', ' ').replace('[', '(').replace(']', ')')
        lines.append(f"- [[{target_slug}|{safe_name}]]{marker}".rstrip())
    lines.append('')
    return '\n'.join(lines)


def main() -> int:
    clusters = load_clusters_meta()
    scotus_set = set(clusters.keys())
    id_by_cluster = {cid: page_id(cid, m) for cid, m in clusters.items()}

    # Cluster→slug for all SCOTUS clusters (what enrich uses to resolve anchors)
    cluster_slug = {cid: id_by_cluster[cid] for cid in scotus_set}

    o2c_all = load_global_o2c()
    edges = load_edges()

    # Outgoing edge list keyed by citing cluster.
    outgoing: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for a, b, d in edges:
        outgoing[a].append((b, d))

    per_cluster = stream_opinions_by_cluster(scotus_set)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    # Prepare work units: one per cluster-with-opinions. Non-opinion clusters
    # still get rewritten (header + cites), just no body.
    work = [(cid, per_cluster.get(cid, [])) for cid in scotus_set]

    log(f"converting bodies with {mp.cpu_count()} workers")
    t0 = time.time()
    written = 0
    with mp.Pool(
        processes=mp.cpu_count(),
        initializer=_init_worker,
        initargs=(o2c_all, cluster_slug),
    ) as pool:
        for cid, body in pool.imap_unordered(convert_cluster, work, chunksize=256):
            meta = clusters.get(cid, {})
            header = build_header(cid, meta)
            footer = build_cites_footer(cid, outgoing, clusters, id_by_cluster)
            parts = [header]
            if body:
                parts.append(body)
            if footer:
                parts.append(footer)
            md = '\n'.join(parts)
            (OUT_DIR / f"{id_by_cluster[cid]}.md").write_text(md)
            written += 1
            if written % 20_000 == 0:
                rate = written / max(time.time() - t0, 1)
                log(f"    … wrote {written:>8,} ({rate:,.0f}/s)")
    log(f"done: wrote {written:,} pages in {time.time() - t0:.1f}s")
    return 0


if __name__ == '__main__':
    sys.exit(main())
