//! End-to-end SCOTUS corpus enricher — single binary, parallel at every stage.
//!
//! Pipeline:
//!   [lbzip2 -dc] → [CSV parse, main thread] → [rayon: HTML → markdown] →
//!     [DashMap<cluster_id, Vec<opinion>>] → [rayon: compose + write .md files]
//!
//! Why this shape:
//! - `libbz2` is single-threaded. `lbzip2` parallelizes across bz2 blocks
//!   (~1.7 GB/s on this box vs ~27 MB/s for the `bzip2` crate). The factor
//!   that dominates everything else is parallel decode.
//! - CSV parse is serial by nature (records can span buffer boundaries).
//!   It runs on one thread, batches SCOTUS rows, dispatches to rayon.
//! - HTML → markdown is pure function per opinion: perfect rayon work.
//! - Writing 495k .md files is filesystem-bound; we parallelize that too.
//!
//! Replaces: `extract_opinion_bodies` (Rust) + `enrich_scotus_bodies.py`.
//! Produces:
//!   wiki-corpus/scotus/<slug-cid>.md  (rewritten in place)
//!   .cache/opinion_to_cluster_all.csv (side effect; streaming write)
//!
//! Usage:
//!   enrich_scotus \
//!     --opinions wiki-corpus/courtlistener-raw/opinions.csv.bz2 \
//!     --scotus-clusters wiki-corpus/courtlistener-raw/.cache/scotus_cluster_ids.txt \
//!     --clusters-meta wiki-corpus/courtlistener-raw/.cache/clusters_meta_583840.json \
//!     --edges wiki-corpus/courtlistener-raw/.cache/edges_495297.csv.gz \
//!     --out-o2c wiki-corpus/courtlistener-raw/.cache/opinion_to_cluster_all.csv \
//!     --out-dir wiki-corpus/scotus

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use ahash::{AHashMap, AHashSet};
use dashmap::DashMap;
use flate2::read::GzDecoder;
use rayon::prelude::*;
use regex::Regex;

// ─── CLI ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Args {
    opinions: PathBuf,
    scotus_clusters: PathBuf,
    clusters_meta: PathBuf,
    edges: PathBuf,
    out_o2c: PathBuf,
    out_dir: PathBuf,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let v = it.next().unwrap_or_else(|| die(&format!("missing value for {arg}")));
        let path = PathBuf::from(v);
        match arg.as_str() {
            "--opinions" => a.opinions = path,
            "--scotus-clusters" => a.scotus_clusters = path,
            "--clusters-meta" => a.clusters_meta = path,
            "--edges" => a.edges = path,
            "--out-o2c" => a.out_o2c = path,
            "--out-dir" => a.out_dir = path,
            other => die(&format!("unknown arg: {other}")),
        }
    }
    for (name, p) in [
        ("--opinions", &a.opinions),
        ("--scotus-clusters", &a.scotus_clusters),
        ("--clusters-meta", &a.clusters_meta),
        ("--edges", &a.edges),
        ("--out-o2c", &a.out_o2c),
        ("--out-dir", &a.out_dir),
    ] {
        if p.as_os_str().is_empty() {
            die(&format!("required: {name}"));
        }
    }
    a
}

fn die(msg: &str) -> ! {
    eprintln!("usage: enrich_scotus --opinions <...> --scotus-clusters <...> --clusters-meta <...> --edges <...> --out-o2c <...> --out-dir <...>");
    eprintln!("error: {msg}");
    std::process::exit(2);
}

// ─── slug (matches tools/ingest_courtlistener.py::slugify) ──────────────────

fn slugify(s: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let capped: String = trimmed.chars().take(max_len).collect();
    if capped.is_empty() {
        "case".to_string()
    } else {
        capped
    }
}

fn page_id(cid: u64, meta: &ClusterMeta) -> String {
    let base_src = meta
        .case_name_short
        .as_deref()
        .filter(|s| !s.is_empty())
        .or(meta.case_name.as_deref())
        .unwrap_or("");
    let base = if base_src.is_empty() {
        format!("case-{cid}")
    } else {
        slugify(base_src, 80)
    };
    format!("{base}-{cid}")
}

// ─── Cluster metadata (subset we care about) ────────────────────────────────

#[derive(Debug, Default, Clone)]
struct ClusterMeta {
    case_name: Option<String>,
    case_name_short: Option<String>,
    date_filed: Option<String>,
    citation_count: u64,
    precedential_status: Option<String>,
}

/// Parse `clusters_meta_583840.json` — a large JSON object keyed by
/// cluster_id (as a string). We only need five fields per entry, so a
/// streaming parser would be ideal — but for this one-shot run, loading
/// the 107 MB JSON with `serde_json::from_reader` into `Value` then
/// lowering to `ClusterMeta` takes ~5 s and keeps the code boring.
fn load_clusters_meta(path: &Path) -> std::io::Result<AHashMap<u64, ClusterMeta>> {
    eprintln!(
        "[enrich] loading cluster metadata from {} ({} MiB)",
        path.display(),
        path.metadata().map(|m| m.len() >> 20).unwrap_or(0)
    );
    let t0 = Instant::now();
    let f = File::open(path)?;
    let r = BufReader::with_capacity(4 << 20, f);

    // serde_json's Value is slow at this size but avoids hand-rolling a parser.
    let v: serde_json::Value = serde_json::from_reader(r)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let obj = v.as_object().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "clusters meta not an object")
    })?;

    let mut m: AHashMap<u64, ClusterMeta> = AHashMap::with_capacity(obj.len());
    for (k, val) in obj {
        let cid: u64 = match k.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let entry = ClusterMeta {
            case_name: val.get("case_name").and_then(|v| v.as_str()).map(str::to_owned),
            case_name_short: val.get("case_name_short").and_then(|v| v.as_str()).map(str::to_owned),
            date_filed: val.get("date_filed").and_then(|v| v.as_str()).map(str::to_owned),
            citation_count: val.get("citation_count").and_then(|v| v.as_u64()).unwrap_or(0),
            precedential_status: val.get("precedential_status").and_then(|v| v.as_str()).map(str::to_owned),
        };
        m.insert(cid, entry);
    }
    eprintln!("[enrich]   {} clusters in {:.1}s", m.len(), t0.elapsed().as_secs_f64());
    Ok(m)
}

fn load_u64_set(p: &Path) -> std::io::Result<AHashSet<u64>> {
    let f = File::open(p)?;
    let r = BufReader::with_capacity(1 << 20, f);
    let mut set = AHashSet::with_capacity(1 << 20);
    for line in r.lines() {
        let s = line?;
        if let Ok(n) = s.trim().parse::<u64>() {
            set.insert(n);
        }
    }
    Ok(set)
}

fn load_edges(p: &Path) -> std::io::Result<Vec<(u64, u64, u32)>> {
    let f = File::open(p)?;
    let r = BufReader::with_capacity(1 << 20, f);
    let gz = GzDecoder::new(r);
    let r = BufReader::new(gz);
    let mut out = Vec::with_capacity(1 << 20);
    for line in r.lines() {
        let line = line?;
        let mut it = line.split(',');
        let a: u64 = match it.next().and_then(|s| s.parse().ok()) { Some(v) => v, None => continue };
        let b: u64 = match it.next().and_then(|s| s.parse().ok()) { Some(v) => v, None => continue };
        let d: u32 = match it.next().and_then(|s| s.parse().ok()) { Some(v) => v, None => continue };
        out.push((a, b, d));
    }
    Ok(out)
}

// ─── HTML → markdown ────────────────────────────────────────────────────────

struct Converter {
    rx_cite: Regex,
    rx_block_breaks: Regex,
    rx_tag_any: Regex,
    rx_ws_runs: Regex,
    rx_multi_blanks: Regex,
}

impl Converter {
    fn new() -> Self {
        Self {
            rx_cite: Regex::new(r#"(?is)<a\s+[^>]*?href="/opinion/(\d+)/[^"]*"[^>]*>(.*?)</a>"#).unwrap(),
            rx_block_breaks: Regex::new(r"(?i)</?(?:p|div|center|blockquote|ol|ul|li|br|hr|h[1-6]|pre|table|tr)\b[^>]*>").unwrap(),
            rx_tag_any: Regex::new(r"<[^>]+>").unwrap(),
            rx_ws_runs: Regex::new(r"[ \t]+").unwrap(),
            rx_multi_blanks: Regex::new(r"\n{3,}").unwrap(),
        }
    }

    fn convert(
        &self,
        html: &str,
        o2c_all: &DashMap<u64, u64>,
        cluster_slug: &AHashMap<u64, String>,
    ) -> String {
        // 1. Resolve citation anchors → wiki-links or plain text.
        let mut out = String::with_capacity(html.len());
        let mut last = 0usize;
        for cap in self.rx_cite.captures_iter(html) {
            let m = cap.get(0).unwrap();
            out.push_str(&html[last..m.start()]);
            let opinion_id: Option<u64> = cap.get(1).and_then(|g| g.as_str().parse().ok());
            let anchor_inner = cap.get(2).map(|g| g.as_str()).unwrap_or("");
            let anchor_text = self.rx_tag_any.replace_all(anchor_inner, "");
            let anchor_text = anchor_text.trim();

            let linked: Option<&str> = opinion_id
                .and_then(|oid| o2c_all.get(&oid).map(|e| *e.value()))
                .and_then(|cid| cluster_slug.get(&cid).map(String::as_str));

            match linked {
                Some(slug) => {
                    let safe = anchor_text
                        .replace('|', " ")
                        .replace('[', "(")
                        .replace(']', ")");
                    let display = if safe.is_empty() { slug.to_string() } else { safe };
                    out.push_str("[[");
                    out.push_str(slug);
                    out.push('|');
                    out.push_str(&display);
                    out.push_str("]]");
                }
                None => {
                    out.push_str(anchor_text);
                }
            }
            last = m.end();
        }
        out.push_str(&html[last..]);

        // 2. Block-level tags → newlines (preserve paragraph structure).
        let out = self.rx_block_breaks.replace_all(&out, "\n");
        // 3. Everything else → gone.
        let out = self.rx_tag_any.replace_all(&out, "");
        // 4. HTML entity decode.
        let out = decode_entities(&out);
        // 5. Whitespace normalization.
        let out = self.rx_ws_runs.replace_all(&out, " ");
        let out = self.rx_multi_blanks.replace_all(&out, "\n\n");
        out.trim().to_string()
    }
}

/// Minimal HTML entity decoder for the ones we actually see in CourtListener
/// exports. Full `html_escape::decode_html_entities` would pull a dep; the
/// long tail of named entities doesn't appear in opinion prose.
fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Scan for ending semicolon up to 10 chars away.
        let end = bytes[i..].iter().take(12).position(|&c| c == b';');
        let Some(end_off) = end else {
            out.push('&');
            i += 1;
            continue;
        };
        let entity = std::str::from_utf8(&bytes[i + 1..i + end_off]).unwrap_or("");
        let decoded: Option<char> = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "mdash" => Some('—'),
            "ndash" => Some('–'),
            "hellip" => Some('…'),
            "sect" => Some('§'),
            "para" => Some('¶'),
            e if e.starts_with("#x") || e.starts_with("#X") => {
                u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32)
            }
            e if e.starts_with('#') => {
                e[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                i += end_off + 1;
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

// ─── Opinion type ordering ──────────────────────────────────────────────────

fn type_heading(t: &str) -> &'static str {
    match t {
        "010combined" | "015unamimous" | "020lead" => "Opinion",
        "025plurality" => "Plurality Opinion",
        "030concurrence" => "Concurrence",
        "035concurrenceinpart" => "Concurrence in Part",
        "040dissent" => "Dissent",
        "050addendum" => "Addendum",
        "060remittitur" => "Remittitur",
        "070rehearing" => "On Rehearing",
        "080onthemerits" => "On the Merits",
        "090onmotiontostrike" => "On Motion to Strike",
        "100trialcourt" => "Trial Court Opinion",
        _ => "Opinion",
    }
}

fn type_order(t: &str) -> u8 {
    match t {
        "010combined" | "015unamimous" | "020lead" => 0,
        "025plurality" => 1,
        "030concurrence" | "035concurrenceinpart" => 2,
        "040dissent" => 3,
        "050addendum" => 4,
        "060remittitur" => 5,
        "070rehearing" => 6,
        "080onthemerits" => 7,
        "090onmotiontostrike" => 8,
        "100trialcourt" => 9,
        _ => 99,
    }
}

// ─── Main driver ────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args();

    // Phase 0 — load auxiliary metadata (fast)
    let scotus_clusters = load_u64_set(&args.scotus_clusters)?;
    eprintln!("[enrich] SCOTUS clusters: {}", scotus_clusters.len());
    let clusters_meta = load_clusters_meta(&args.clusters_meta)?;
    let edges = load_edges(&args.edges)?;
    eprintln!("[enrich] edges loaded: {}", edges.len());

    // Build cluster_id → slug (Arc for workers) and outgoing adj list.
    let cluster_slug: AHashMap<u64, String> = scotus_clusters
        .iter()
        .filter_map(|&cid| clusters_meta.get(&cid).map(|m| (cid, page_id(cid, m))))
        .collect();
    eprintln!("[enrich] slug table: {}", cluster_slug.len());

    let mut outgoing: HashMap<u64, Vec<(u64, u32)>> = HashMap::new();
    for (a, b, d) in &edges {
        outgoing.entry(*a).or_default().push((*b, *d));
    }
    for list in outgoing.values_mut() {
        list.sort_by(|a, b| b.1.cmp(&a.1));
        list.truncate(200);
    }

    // Phase 1 — spawn lbzip2 → CSV parse → dispatch SCOTUS rows to rayon.
    eprintln!("[enrich] spawning lbzip2 -dc {}", args.opinions.display());
    let mut child = Command::new("lbzip2")
        .args(["-dc", args.opinions.to_str().unwrap()])
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("failed to capture lbzip2 stdout")?;
    let bufread = BufReader::with_capacity(32 << 20, stdout);

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .escape(Some(b'\\'))
        .double_quote(false)
        .from_reader(bufread);
    let hdr = reader.headers()?.clone();
    let find = |n: &str| hdr.iter().position(|h| h == n).expect("missing column");
    let i_id = find("id");
    let i_cluster = find("cluster_id");
    let i_type = find("type");
    let i_hw = find("html_with_citations");
    eprintln!(
        "[enrich] columns: id@{} cluster_id@{} type@{} html_with_citations@{}",
        i_id, i_cluster, i_type, i_hw
    );

    let o2c_all: Arc<DashMap<u64, u64>> = Arc::new(DashMap::with_capacity(16 << 20));
    let opinions_by_cluster: Arc<DashMap<u64, Vec<(u64, String, String)>>> =
        Arc::new(DashMap::with_capacity(500_000));

    let converter = Arc::new(Converter::new());
    let cluster_slug_arc = Arc::new(cluster_slug);

    let rows_total = AtomicU64::new(0);
    let bodies_total = AtomicU64::new(0);
    let bytes_total = AtomicU64::new(0);
    let t_stream = Instant::now();

    // Batch SCOTUS rows, par_iter each batch, keep the CSV reader hot.
    const BATCH: usize = 256;
    let mut batch: Vec<(u64, u64, String, String)> = Vec::with_capacity(BATCH);

    let mut o2c_out = BufWriter::with_capacity(8 << 20, File::create(&args.out_o2c)?);

    let drain_batch = |batch: &mut Vec<(u64, u64, String, String)>,
                       conv: &Converter,
                       o2c: &DashMap<u64, u64>,
                       slug: &AHashMap<u64, String>,
                       out: &DashMap<u64, Vec<(u64, String, String)>>,
                       bodies_total: &AtomicU64,
                       bytes_total: &AtomicU64| {
        if batch.is_empty() {
            return;
        }
        let drained: Vec<_> = batch.drain(..).collect();
        let done = drained
            .into_par_iter()
            .map(|(id, cid, ty, html)| {
                let bytes = html.len() as u64;
                let md = conv.convert(&html, o2c, slug);
                (id, cid, ty, md, bytes)
            })
            .collect::<Vec<_>>();
        for (id, cid, ty, md, bytes) in done {
            out.entry(cid).or_default().push((id, ty, md));
            bodies_total.fetch_add(1, Ordering::Relaxed);
            bytes_total.fetch_add(bytes, Ordering::Relaxed);
        }
    };

    for result in reader.records() {
        let row = match result { Ok(r) => r, Err(_) => continue };
        let id = match row.get(i_id).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v, None => continue,
        };
        let cid = match row.get(i_cluster).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v, None => continue,
        };

        writeln!(o2c_out, "{},{}", id, cid)?;
        o2c_all.insert(id, cid);

        let r = rows_total.fetch_add(1, Ordering::Relaxed) + 1;

        if scotus_clusters.contains(&cid) {
            let hw = row.get(i_hw).unwrap_or("");
            if !hw.is_empty() {
                let ty = row.get(i_type).unwrap_or("").to_string();
                batch.push((id, cid, ty, hw.to_string()));
                if batch.len() >= BATCH {
                    drain_batch(
                        &mut batch,
                        &converter,
                        &o2c_all,
                        &cluster_slug_arc,
                        &opinions_by_cluster,
                        &bodies_total,
                        &bytes_total,
                    );
                }
            }
        }

        if r % 200_000 == 0 {
            let el = t_stream.elapsed().as_secs_f64();
            eprintln!(
                "[enrich] {:>10} rows  bodies={}  bytes={} MiB  {:.0} rows/s  ({:.0} s)",
                r,
                bodies_total.load(Ordering::Relaxed),
                bytes_total.load(Ordering::Relaxed) >> 20,
                r as f64 / el.max(1e-6),
                el,
            );
        }
    }

    drain_batch(
        &mut batch,
        &converter,
        &o2c_all,
        &cluster_slug_arc,
        &opinions_by_cluster,
        &bodies_total,
        &bytes_total,
    );
    o2c_out.flush()?;
    let _ = child.wait();

    let el = t_stream.elapsed().as_secs_f64();
    eprintln!(
        "[enrich] STREAM DONE: {} rows, bodies={}, bytes={} MiB in {:.1}s ({:.0} rows/s)",
        rows_total.load(Ordering::Relaxed),
        bodies_total.load(Ordering::Relaxed),
        bytes_total.load(Ordering::Relaxed) >> 20,
        el,
        rows_total.load(Ordering::Relaxed) as f64 / el.max(1e-6),
    );

    // Phase 2 — parallel write of .md files.
    std::fs::create_dir_all(&args.out_dir)?;
    eprintln!("[enrich] writing .md files to {}", args.out_dir.display());
    let t_write = Instant::now();
    let written = AtomicU64::new(0);
    let out_dir = args.out_dir.clone();

    let scotus_vec: Vec<u64> = scotus_clusters.iter().copied().collect();
    scotus_vec.par_iter().for_each(|&cid| {
        let meta = match clusters_meta.get(&cid) { Some(m) => m, None => return };
        let slug = match cluster_slug_arc.get(&cid) { Some(s) => s, None => return };

        // Header
        let mut md = String::with_capacity(4096);
        let name = meta
            .case_name
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(meta.case_name_short.as_deref())
            .unwrap_or("")
            .trim();
        md.push_str("# ");
        if name.is_empty() {
            md.push_str(&format!("Case {}", cid));
        } else {
            md.push_str(name);
        }
        md.push_str("\n\n");
        let mut bits: Vec<String> = Vec::new();
        if let Some(d) = meta.date_filed.as_deref().filter(|s| !s.is_empty()) {
            bits.push(format!("filed {d}"));
        }
        if let Some(s) = meta.precedential_status.as_deref().filter(|s| !s.is_empty()) {
            bits.push(s.to_lowercase());
        }
        if meta.citation_count > 0 {
            bits.push(format!("{} incoming citations", meta.citation_count));
        }
        if !bits.is_empty() {
            md.push('*');
            md.push_str(&bits.join(", "));
            md.push_str("*\n\n");
        }

        // Body
        if let Some(mut opinions) = opinions_by_cluster.get_mut(&cid) {
            opinions.sort_by_key(|(id, t, _)| (type_order(t), *id));
            for (_, t, body) in opinions.iter() {
                let heading = type_heading(t);
                md.push_str("## ");
                md.push_str(heading);
                md.push_str("\n\n");
                md.push_str(body);
                md.push_str("\n\n");
            }
        }

        // Footer: outgoing citations
        if let Some(cites) = outgoing.get(&cid) {
            md.push_str("## Cites (");
            md.push_str(&cites.len().to_string());
            md.push_str(")\n\n");
            for (target_cid, depth) in cites.iter() {
                let Some(target_slug) = cluster_slug_arc.get(target_cid) else { continue };
                let target_name = clusters_meta.get(target_cid)
                    .and_then(|m| m.case_name_short.as_deref().or(m.case_name.as_deref()))
                    .unwrap_or("")
                    .trim()
                    .replace('|', " ")
                    .replace('[', "(")
                    .replace(']', ")");
                let marker = if *depth > 1 { format!(" ×{depth}") } else { String::new() };
                md.push_str("- [[");
                md.push_str(target_slug);
                md.push('|');
                md.push_str(&target_name);
                md.push_str("]]");
                md.push_str(&marker);
                md.push('\n');
            }
            md.push('\n');
        }

        let fname = format!("{slug}.md");
        let path = out_dir.join(&fname);
        if let Err(e) = std::fs::write(&path, md.as_bytes()) {
            eprintln!("[enrich] WRITE FAIL {}: {e}", path.display());
            return;
        }
        let w = written.fetch_add(1, Ordering::Relaxed) + 1;
        if w % 50_000 == 0 {
            eprintln!("[enrich]   wrote {} pages", w);
        }
    });

    eprintln!(
        "[enrich] WRITE DONE: {} pages in {:.1}s",
        written.load(Ordering::Relaxed),
        t_write.elapsed().as_secs_f64()
    );
    eprintln!("[enrich] total elapsed {:.1}s", t_stream.elapsed().as_secs_f64());
    Ok(())
}
