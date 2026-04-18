#!/usr/bin/env python3
"""
Ingest CourtListener bulk data into cowiki-rs markdown format.

Stages:
  A  dockets   → court_id per docket_id        (fast, filters to target jurisdiction)
  B  clusters  → case metadata per cluster_id  (fast)
  C  opinions  → opinion_id → cluster_id       (slow — the 51 GiB file)
  D  citations → aggregated cluster→cluster    (slow — ~70M edges)
  E  emit      → one .md per cluster in target

Usage:
  python3 tools/ingest_courtlistener.py --court scotus --out wiki-corpus/scotus
  python3 tools/ingest_courtlistener.py --court scotus --out wiki-corpus/scotus --stage all

Each stage writes intermediate artefacts to a cache dir (default
`wiki-corpus/courtlistener-raw/.cache/`) so reruns skip completed work.
"""

from __future__ import annotations
import argparse
import bz2
import csv
import gzip
import json
import os
import sys
import time
from collections import defaultdict
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "wiki-corpus" / "courtlistener-raw"
CACHE = RAW / ".cache"

# Bump Python's CSV field limit — opinion plain_text is enormous.
csv.field_size_limit(sys.maxsize)


# ──────────────── helpers ──────────────────────────────────────────────────

def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def slugify(s: str, max_len: int = 80) -> str:
    """CourtListener case names → kebab-case page IDs."""
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


# ──────────────── stage A: dockets → court_id ──────────────────────────────

def stage_dockets(target_court: str | None) -> dict[int, str]:
    """Return docket_id → court_id, optionally filtered to one court."""
    cache_path = CACHE / f"dockets_court_{target_court or 'all'}.json"
    if cache_path.exists():
        log(f"  loading cached {cache_path.name}")
        return {int(k): v for k, v in json.loads(cache_path.read_text()).items()}

    log(f"  parsing dockets.csv.bz2 (filter: court={target_court or 'ALL'})")
    out: dict[int, str] = {}
    with bz2.open(RAW / "dockets.csv.bz2", "rt", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        n = 0
        for row in reader:
            n += 1
            court = row.get("court_id", "")
            if target_court is None or court == target_court:
                out[int(row["id"])] = court
            if n % 500_000 == 0:
                log(f"    … {n:>10} dockets scanned, {len(out):>6} kept")
    log(f"  dockets: {n:,} scanned, {len(out):,} kept for {target_court or 'ALL'}")
    CACHE.mkdir(parents=True, exist_ok=True)
    cache_path.write_text(json.dumps(out, separators=(",", ":")))
    return out


# ──────────────── stage B: clusters → metadata ─────────────────────────────

def stage_clusters(docket_filter: dict[int, str] | None) -> dict[int, dict]:
    """Return cluster_id → metadata dict, filtered to dockets in `docket_filter`."""
    cache_path = CACHE / f"clusters_meta_{len(docket_filter or {})}.json"
    if cache_path.exists():
        log(f"  loading cached {cache_path.name}")
        return {int(k): v for k, v in json.loads(cache_path.read_text()).items()}

    want = set(docket_filter.keys()) if docket_filter is not None else None
    log("  parsing opinion-clusters.csv.bz2")
    out: dict[int, dict] = {}
    with bz2.open(RAW / "opinion-clusters.csv.bz2", "rt", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        n = 0
        for row in reader:
            n += 1
            did_s = row.get("docket_id") or ""
            id_s = row.get("id") or ""
            if not did_s or not id_s:
                continue  # malformed or ragged row
            try:
                did = int(did_s)
                cid = int(id_s)
            except ValueError:
                continue
            if want is not None and did not in want:
                continue
            # Defensive int parse — CourtListener CSV occasionally drops commas
            # inside free-text fields without quoting (especially for historical
            # records), so typed columns can contain garbage. Skip rather than
            # crash.
            try:
                cc = int((row.get("citation_count") or "0").strip())
            except ValueError:
                cc = 0
            out[cid] = {
                "docket_id": did,
                "case_name": (row.get("case_name") or row.get("case_name_short") or "").strip(),
                "case_name_short": (row.get("case_name_short") or "").strip(),
                "date_filed": row.get("date_filed") or "",
                "citation_count": cc,
                "precedential_status": row.get("precedential_status") or "",
                "slug": row.get("slug") or "",
            }
            if n % 200_000 == 0:
                log(f"    … {n:>10} clusters scanned, {len(out):>6} kept")
    log(f"  clusters: {n:,} scanned, {len(out):,} kept")
    cache_path.write_text(json.dumps(out, separators=(",", ":")))
    return out


# ──────────────── stage C: opinion_id → cluster_id ─────────────────────────
#
# opinions.csv.bz2 is the 51 GiB monster. We only need two columns:
# `id` (column 1) and `cluster_id` (column 15 per schema). Everything else
# is full text we'd throw away. Stream through with csv.reader, keep only
# the columns we need. Result is a compact dict of ~9M entries, ~100 MiB on
# disk.

def stage_opinion_to_cluster(cluster_filter: set[int] | None) -> dict[int, int]:
    cache_path = CACHE / f"opinion_to_cluster_{len(cluster_filter or set())}.csv.gz"
    if cache_path.exists():
        log(f"  loading cached {cache_path.name}")
        out: dict[int, int] = {}
        with gzip.open(cache_path, "rt") as f:
            for line in f:
                oid, cid = line.rstrip().split(",")
                out[int(oid)] = int(cid)
        return out

    log("  parsing opinions.csv.bz2 (stream, keep id+cluster_id only)")
    out: dict[int, int] = {}
    with bz2.open(RAW / "opinions.csv.bz2", "rt", encoding="utf-8", newline="") as f:
        reader = csv.reader(f)
        header = next(reader)
        try:
            id_col = header.index("id")
            cluster_col = header.index("cluster_id")
        except ValueError:
            raise RuntimeError(f"expected id+cluster_id in header, got {header[:8]}…")

        n = 0
        t0 = time.time()
        for row in reader:
            n += 1
            try:
                oid = int(row[id_col])
                cid = int(row[cluster_col])
            except (IndexError, ValueError):
                continue
            if cluster_filter is None or cid in cluster_filter:
                out[oid] = cid
            if n % 500_000 == 0:
                rate = n / max(time.time() - t0, 1)
                log(f"    … {n:>10} opinions scanned ({rate:,.0f}/s), {len(out):>6} kept")
    log(f"  opinions: {n:,} scanned, {len(out):,} kept")
    CACHE.mkdir(parents=True, exist_ok=True)
    with gzip.open(cache_path, "wt") as f:
        for oid, cid in out.items():
            f.write(f"{oid},{cid}\n")
    return out


# ──────────────── stage D: citation-map → cluster→cluster edges ────────────

def stage_citations(
    opinion_to_cluster: dict[int, int],
    cluster_set: set[int],
) -> list[tuple[int, int, int]]:
    """Aggregate opinion→opinion citations to cluster→cluster edges.
    Only keep edges where both endpoints are in `cluster_set`. Depth is summed
    over all constituent opinion citations.
    """
    cache_path = CACHE / f"edges_{len(cluster_set)}.csv.gz"
    if cache_path.exists():
        log(f"  loading cached {cache_path.name}")
        out: list[tuple[int, int, int]] = []
        with gzip.open(cache_path, "rt") as f:
            for line in f:
                a, b, d = line.rstrip().split(",")
                out.append((int(a), int(b), int(d)))
        return out

    log("  parsing citation-map.csv.bz2 (stream, aggregate at cluster level)")
    agg: dict[tuple[int, int], int] = defaultdict(int)
    with bz2.open(RAW / "citation-map.csv.bz2", "rt", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        n = 0
        kept = 0
        t0 = time.time()
        for row in reader:
            n += 1
            try:
                citing = int(row["citing_opinion_id"])
                cited = int(row["cited_opinion_id"])
                depth = int(row["depth"])
            except (ValueError, KeyError):
                continue
            cc = opinion_to_cluster.get(citing)
            tc = opinion_to_cluster.get(cited)
            if cc is None or tc is None or cc == tc:
                continue
            if cc not in cluster_set or tc not in cluster_set:
                continue
            agg[(cc, tc)] += depth
            kept += 1
            if n % 1_000_000 == 0:
                rate = n / max(time.time() - t0, 1)
                log(f"    … {n:>10} rows ({rate:,.0f}/s), {kept:>6} kept, {len(agg):>6} unique edges")
    log(f"  citations: {n:,} rows → {len(agg):,} unique cluster→cluster edges")
    out_list = [(a, b, d) for (a, b), d in agg.items()]
    with gzip.open(cache_path, "wt") as f:
        for a, b, d in out_list:
            f.write(f"{a},{b},{d}\n")
    return out_list


# ──────────────── stage E: emit markdown ───────────────────────────────────

def stage_emit(
    out_dir: Path,
    clusters: dict[int, dict],
    edges: list[tuple[int, int, int]],
) -> None:
    log(f"  emitting {len(clusters):,} .md files into {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)

    # Build outgoing edge list per cluster.
    outgoing: dict[int, list[tuple[int, int]]] = defaultdict(list)
    for citing, cited, depth in edges:
        outgoing[citing].append((cited, depth))

    # For each cluster, derive a stable page id (cluster id → slug-{id}).
    # Using the numeric id in the filename guarantees uniqueness even when
    # case_name_short collides (it does a lot — "In re Smith" etc).
    def page_id(cid: int, meta: dict) -> str:
        base = slugify(meta.get("case_name_short") or meta.get("case_name") or f"case-{cid}")
        return f"{base}-{cid}"

    id_by_cluster = {cid: page_id(cid, m) for cid, m in clusters.items()}

    written = 0
    for cid, meta in clusters.items():
        pid = id_by_cluster[cid]
        name = meta.get("case_name") or meta.get("case_name_short") or f"Case {cid}"
        date = meta.get("date_filed", "")
        status = meta.get("precedential_status", "")
        incoming_count = meta.get("citation_count", 0)

        body_lines = [f"# {name}", ""]
        bits = []
        if date: bits.append(f"filed {date}")
        if status: bits.append(status.lower())
        if incoming_count: bits.append(f"{incoming_count} incoming citations")
        if bits:
            body_lines.append("*" + ", ".join(bits) + "*")
            body_lines.append("")

        cites = outgoing.get(cid, [])
        if cites:
            # Sort by depth desc — the most-relied-on citations bubble up.
            cites.sort(key=lambda x: x[1], reverse=True)
            body_lines.append(f"## Cites ({len(cites)})")
            body_lines.append("")
            for target_cid, depth in cites[:200]:  # cap per page; full list would be n² worst-case
                target_pid = id_by_cluster.get(target_cid)
                if target_pid:
                    target_name = clusters[target_cid].get("case_name_short") or clusters[target_cid].get("case_name", "")
                    marker = f"×{depth}" if depth > 1 else ""
                    body_lines.append(f"- [[{target_pid}|{target_name}]] {marker}".rstrip())
            body_lines.append("")

        (out_dir / f"{pid}.md").write_text("\n".join(body_lines))
        written += 1
        if written % 5_000 == 0:
            log(f"    … wrote {written:,}")
    log(f"  emit: {written:,} pages")


# ──────────────── driver ───────────────────────────────────────────────────

def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--court", default="scotus",
                    help="court_id to filter to, or 'all' for everything")
    ap.add_argument("--out", required=True, help="output directory for .md files")
    ap.add_argument("--stage", default="all",
                    choices=["dockets", "clusters", "opinions", "citations", "emit", "all"])
    args = ap.parse_args()

    target_court = None if args.court == "all" else args.court
    out_dir = Path(args.out).resolve()

    CACHE.mkdir(parents=True, exist_ok=True)

    if args.stage in ("dockets", "clusters", "opinions", "citations", "emit", "all"):
        log("stage A: dockets")
        dockets = stage_dockets(target_court)

    if args.stage == "dockets":
        return 0

    if args.stage in ("clusters", "opinions", "citations", "emit", "all"):
        log("stage B: clusters")
        clusters = stage_clusters(dockets if target_court else None)

    if args.stage == "clusters":
        return 0

    if args.stage in ("opinions", "citations", "emit", "all"):
        log("stage C: opinions → cluster map")
        o2c = stage_opinion_to_cluster(set(clusters.keys()) if target_court else None)

    if args.stage == "opinions":
        return 0

    if args.stage in ("citations", "emit", "all"):
        log("stage D: citation-map → cluster edges")
        edges = stage_citations(o2c, set(clusters.keys()))

    if args.stage == "citations":
        return 0

    log("stage E: emit markdown")
    stage_emit(out_dir, clusters, edges)
    log("done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
