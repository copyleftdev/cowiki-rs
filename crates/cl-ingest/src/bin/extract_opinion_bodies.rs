//! One-pass extractor over the 54 GiB `opinions.csv.bz2`. Produces two
//! artefacts at once so we don't have to stream the file twice:
//!
//!   1. `.cache/opinion_to_cluster_all.csv` — every opinion's cluster_id,
//!      not filtered to a court. We need the full map to resolve in-text
//!      citation anchors (`<a href="/opinion/N/..."`) to cluster ids.
//!
//!   2. `.cache/scotus_opinion_bodies.jsonl.gz` — one line per SCOTUS
//!      opinion: `{"id":N,"cid":M,"type":"...","html":"..."}`. `html` is
//!      `html_with_citations`, where CourtListener has already resolved
//!      every in-text citation to `<a href="/opinion/.../...">`. That's
//!      how we rebuild the article bodies with real wiki-links.
//!
//! Decompression is single-threaded (libbz2 constraint). CSV parse and
//! output are on the same thread for now — the bottleneck is bz2 decode
//! (~30 MB/s) regardless. Runs end-to-end in ~30 min on this box.
//!
//! Usage:
//!   extract_opinion_bodies <opinions.csv.bz2> <scotus_cluster_ids.txt> \
//!     <out_o2c_all.csv> <out_bodies.jsonl.gz>

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use bzip2::read::MultiBzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

fn load_u64_set(p: &Path) -> std::io::Result<HashSet<u64>> {
    let f = File::open(p)?;
    let r = BufReader::with_capacity(1 << 20, f);
    let mut set = HashSet::with_capacity(1 << 20);
    for line in r.lines() {
        let s = line?;
        if let Ok(n) = s.trim().parse::<u64>() {
            set.insert(n);
        }
    }
    Ok(set)
}

/// Escape a string for JSON embedding. Only the characters JSON reserves —
/// we write UTF-8 bytes otherwise. Faster than pulling in serde for this
/// one structurally-trivial record.
fn json_escape_into(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "usage: {} <opinions.csv.bz2> <scotus_cluster_ids.txt> <out_o2c_all.csv> <out_bodies.jsonl.gz>",
            args[0]
        );
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let filter_path = Path::new(&args[2]);
    let out_o2c_path = Path::new(&args[3]);
    let out_bodies_path = Path::new(&args[4]);

    eprintln!("[ext] loading SCOTUS cluster filter from {}", filter_path.display());
    let t0 = Instant::now();
    let scotus: HashSet<u64> = load_u64_set(filter_path)?;
    eprintln!("[ext]   {} cluster ids in {:.1}s", scotus.len(), t0.elapsed().as_secs_f64());

    eprintln!("[ext] streaming {}", input.display());
    let f = File::open(input)?;
    let bz = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, f));
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .escape(Some(b'\\'))
        .double_quote(false)
        .from_reader(bz);

    let hdr = reader.headers()?.clone();
    let find = |name: &str| -> usize {
        hdr.iter()
            .position(|h| h == name)
            .unwrap_or_else(|| panic!("column not found: {name}"))
    };
    let i_id = find("id");
    let i_cluster = find("cluster_id");
    let i_type = find("type");
    let i_hw = find("html_with_citations");
    eprintln!(
        "[ext] columns: id@{} cluster_id@{} type@{} html_with_citations@{}",
        i_id, i_cluster, i_type, i_hw
    );

    let mut o2c_out = BufWriter::with_capacity(1 << 20, File::create(out_o2c_path)?);
    let bodies_raw = File::create(out_bodies_path)?;
    let mut bodies_out = GzEncoder::new(
        BufWriter::with_capacity(1 << 20, bodies_raw),
        Compression::fast(),
    );

    let t_scan = Instant::now();
    let mut rows = 0u64;
    let mut o2c_written = 0u64;
    let mut bodies_written = 0u64;
    let mut bytes_body = 0u64;
    let mut line_buf = String::with_capacity(64 * 1024);

    for result in reader.records() {
        rows += 1;
        let row = match result {
            Ok(r) => r,
            Err(_) => continue,
        };

        let id = match row.get(i_id).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let cluster_id = match row.get(i_cluster).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => continue,
        };

        // Always write the global o2c row.
        writeln!(o2c_out, "{},{}", id, cluster_id)?;
        o2c_written += 1;

        // For SCOTUS clusters, also emit the body.
        if scotus.contains(&cluster_id) {
            let opinion_type = row.get(i_type).unwrap_or("");
            let hw = row.get(i_hw).unwrap_or("");
            if !hw.is_empty() {
                line_buf.clear();
                line_buf.push_str(r#"{"id":"#);
                line_buf.push_str(&id.to_string());
                line_buf.push_str(r#","cid":"#);
                line_buf.push_str(&cluster_id.to_string());
                line_buf.push_str(r#","type":"#);
                json_escape_into(&mut line_buf, opinion_type);
                line_buf.push_str(r#","html":"#);
                json_escape_into(&mut line_buf, hw);
                line_buf.push('}');
                line_buf.push('\n');
                bodies_out.write_all(line_buf.as_bytes())?;
                bodies_written += 1;
                bytes_body += hw.len() as u64;
            }
        }

        if rows % 200_000 == 0 {
            let el = t_scan.elapsed().as_secs_f64();
            eprintln!(
                "[ext] {:>10} rows  o2c={}  bodies={}  body_bytes={:>8} MiB  {:.0} rows/s",
                rows,
                o2c_written,
                bodies_written,
                bytes_body / (1 << 20),
                rows as f64 / el.max(1e-6),
            );
        }
    }

    o2c_out.flush()?;
    bodies_out.finish()?.flush()?;

    let el = t_scan.elapsed().as_secs_f64();
    eprintln!(
        "[ext] DONE: {} rows, o2c={}, bodies={}, body_bytes={} MiB, {:.1}s ({:.0} rows/s)",
        rows,
        o2c_written,
        bodies_written,
        bytes_body / (1 << 20),
        el,
        rows as f64 / el.max(1e-6),
    );

    Ok(())
}
