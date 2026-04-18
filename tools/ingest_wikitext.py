#!/usr/bin/env python3
"""
Ingest a wiki-dat JSONL corpus into wiki-backend-compatible markdown.

Input:  /home/ops/Project/wiki-dat/corpus/<topic>/wiki.jsonl
        (records: {pageid, title, wikitext, categories, ...})

Output: <out>/<topic>/<slug>.md
        with `# Title` header, templates/refs stripped, wikitext links
        preserved as [[target|display]] and filtered to in-corpus targets.

The slug function matches wiki-backend/src/parse.rs::normalize_link exactly
(lowercase, spaces→hyphens) so [[Foo Bar]] in the body and file foo-bar.md
produce the same PageId.

Usage:
    python3 tools/ingest_wikitext.py <topic> [--out wiki-corpus] [--src <jsonl-root>]
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


DEFAULT_SRC = Path("/home/ops/Project/wiki-dat/corpus")
DEFAULT_OUT = Path("wiki-corpus")


def slugify(raw: str) -> str:
    """Match wiki-backend's link + path normalization."""
    s = raw.strip().lower().replace(" ", "-").replace("\\", "/")
    return s.strip("/")


def safe_filename(slug: str) -> str:
    """The filename must round-trip through scan.rs::path_to_page_id (which
    only lowercases) back to exactly `slug`. So we only strip characters the
    filesystem forbids: NUL and internal '/' (which would create subdirs).
    Links in the body are already rewritten to this canonical slug, so
    whatever we keep here is what parse.rs will match on."""
    s = slug.replace("\x00", "").replace("/", "-")
    s = re.sub(r"-+", "-", s).strip("-")
    return s


# ─── Wikitext → Markdown ─────────────────────────────────────────────────────

RE_REF_PAIRED = re.compile(r"<ref\b[^>]*>.*?</ref>", re.DOTALL | re.IGNORECASE)
RE_REF_SELF = re.compile(r"<ref\b[^>]*/>", re.IGNORECASE)
RE_HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
RE_TAG_SIMPLE = re.compile(r"</?(?:small|big|sup|sub|code|nowiki|br|hr)\b[^>]*/?>",
                           re.IGNORECASE)
RE_TABLE = re.compile(r"\{\|.*?\|\}", re.DOTALL)
RE_BOLD = re.compile(r"'''(.+?)'''", re.DOTALL)
RE_ITALIC = re.compile(r"''(.+?)''", re.DOTALL)
RE_HEADING = re.compile(r"^(={2,6})\s*(.+?)\s*\1\s*$", re.MULTILINE)
RE_EXTLINK = re.compile(r"\[(https?://\S+)\s+([^\]]+)\]")
RE_EXTLINK_BARE = re.compile(r"\[(https?://\S+)\]")
RE_WIKILINK = re.compile(r"\[\[([^\[\]]+?)\]\]")
RE_MULTI_BLANK = re.compile(r"\n{3,}")
RE_LEADING_WS = re.compile(r"^[ \t]+", re.MULTILINE)

DROP_NAMESPACES = ("file:", "image:", "category:", "wikipedia:", "help:",
                   "template:", "user:", "talk:", "portal:")


def strip_templates(text: str) -> str:
    """Iteratively remove {{...}} from innermost outward. Handles nesting."""
    inner = re.compile(r"\{\{[^{}]*\}\}", re.DOTALL)
    prev = None
    while prev != text:
        prev = text
        text = inner.sub("", text)
    return text


def replace_heading(match: re.Match) -> str:
    depth = len(match.group(1))
    text = match.group(2).strip()
    # Wiki === -> markdown ## (wiki == is top-level section, map to ## since # is title)
    level = max(2, min(6, depth))
    return f"{'#' * level} {text}"


def rewrite_wikilink(match: re.Match, valid_slugs: set[str]) -> str:
    body = match.group(1).strip()
    if "|" in body:
        target, display = body.split("|", 1)
        target = target.strip()
        display = display.strip()
    else:
        target = body
        display = body

    low = target.lower()
    if any(low.startswith(ns) for ns in DROP_NAMESPACES):
        return ""

    # Strip fragment.
    if "#" in target:
        target = target.split("#", 1)[0].strip()
        if not target:
            return display

    slug = slugify(target)
    if slug in valid_slugs:
        # Rewrite to the canonical slug so the filename and the in-body link
        # normalize to identical PageIds regardless of quirky title punctuation.
        if display and display.lower() != target.lower():
            return f"[[{slug}|{display}]]"
        return f"[[{slug}]]"
    # Not in corpus: drop the link, keep the display text.
    return display


def wikitext_to_markdown(text: str, valid_slugs: set[str]) -> str:
    text = RE_HTML_COMMENT.sub("", text)
    text = RE_REF_PAIRED.sub("", text)
    text = RE_REF_SELF.sub("", text)
    text = RE_TABLE.sub("", text)
    text = strip_templates(text)
    text = RE_TAG_SIMPLE.sub("", text)
    text = RE_HEADING.sub(replace_heading, text)
    text = RE_BOLD.sub(r"**\1**", text)
    text = RE_ITALIC.sub(r"*\1*", text)
    text = RE_EXTLINK.sub(r"[\2](\1)", text)
    text = RE_EXTLINK_BARE.sub(r"\1", text)
    text = RE_WIKILINK.sub(lambda m: rewrite_wikilink(m, valid_slugs), text)
    text = RE_LEADING_WS.sub("", text)
    text = RE_MULTI_BLANK.sub("\n\n", text)
    return text.strip()


# ─── Driver ──────────────────────────────────────────────────────────────────

def is_redirect(wikitext: str) -> bool:
    return bool(re.match(r"\s*#redirect\s*\[\[", wikitext, re.IGNORECASE))


def load_records(jsonl: Path):
    with jsonl.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                continue


def ingest(topic: str, src_root: Path, out_root: Path) -> None:
    jsonl = src_root / topic / "wiki.jsonl"
    if not jsonl.exists():
        sys.exit(f"Source not found: {jsonl}")

    # Pass 1: enumerate articles, build slug set, resolve collisions (first wins).
    records: list[tuple[str, str]] = []  # (slug, raw_record_json)
    slug_set: set[str] = set()
    collisions = 0
    redirects = 0

    for rec in load_records(jsonl):
        title = rec.get("title", "").strip()
        wikitext = rec.get("wikitext", "")
        if not title or not wikitext:
            continue
        if is_redirect(wikitext):
            redirects += 1
            continue
        slug = slugify(title)
        if not slug:
            continue
        if slug in slug_set:
            collisions += 1
            continue
        slug_set.add(slug)
        records.append((slug, json.dumps(rec)))

    print(f"[{topic}] pass1: {len(records)} articles, "
          f"{redirects} redirects skipped, {collisions} collisions dropped",
          file=sys.stderr)

    # Pass 2: convert wikitext, write files, filter links to in-corpus targets.
    out_dir = out_root / topic
    out_dir.mkdir(parents=True, exist_ok=True)
    written = 0
    empty = 0

    for slug, raw in records:
        rec = json.loads(raw)
        title = rec["title"].strip()
        body = wikitext_to_markdown(rec["wikitext"], slug_set)
        if not body:
            empty += 1
            continue
        fn = safe_filename(slug)
        if not fn:
            continue
        path = out_dir / f"{fn}.md"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"# {title}\n\n{body}\n", encoding="utf-8")
        written += 1

    print(f"[{topic}] pass2: wrote {written} pages to {out_dir}, "
          f"{empty} empty after strip", file=sys.stderr)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("topic", help="topic slug (matches dir under src)")
    p.add_argument("--src", type=Path, default=DEFAULT_SRC,
                   help=f"JSONL corpus root (default: {DEFAULT_SRC})")
    p.add_argument("--out", type=Path, default=DEFAULT_OUT,
                   help=f"output markdown root (default: {DEFAULT_OUT})")
    args = p.parse_args()
    ingest(args.topic, args.src, args.out)


if __name__ == "__main__":
    main()
