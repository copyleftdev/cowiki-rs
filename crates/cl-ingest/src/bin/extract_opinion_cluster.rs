//! Stream the CourtListener opinions bulk file, extract (id, cluster_id)
//! rows, write a compact `opinion_id,cluster_id` CSV to stdout (or a file).
//!
//! Replaces the Python stage C which was ~50× slower because Python's csv
//! module is pure-Python and rows have a ~20 KiB `plain_text` column.
//!
//! Usage:
//!   extract_opinion_cluster <opinions.csv.bz2> <out.csv> [--filter <cluster_ids.csv>]
//!
//! If `--filter` is given, only opinions whose `cluster_id` appears in that
//! file (one id per line) are emitted. Used for SCOTUS-only pilots.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use bzip2::read::MultiBzDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <opinions.csv.bz2> <out.csv> [--filter <cluster_ids.csv>]", args[0]);
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let filter_path = args.iter().position(|a| a == "--filter").and_then(|i| args.get(i + 1));

    // Load filter set if provided.
    let filter: Option<HashSet<u64>> = if let Some(p) = filter_path {
        let f = File::open(p)?;
        let r = BufReader::new(f);
        let mut set = HashSet::with_capacity(1024 * 1024);
        for line in r.lines() {
            if let Ok(s) = line {
                if let Ok(n) = s.trim().parse::<u64>() {
                    set.insert(n);
                }
            }
        }
        eprintln!("[extract] filter set: {} cluster ids", set.len());
        Some(set)
    } else {
        None
    };

    // Streaming bz2 → csv. CourtListener's PostgreSQL CSV export uses
    // backslash-escape for quotes inside values (`\"` rather than the
    // standard `""`) — turning `escape` on keeps the csv parser from
    // breaking quote state on every embedded HTML attribute.
    let f = File::open(input)?;
    let bz = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, f));
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .escape(Some(b'\\'))
        .double_quote(false)
        .from_reader(bz);

    // Optional: cap rows read for quick iteration.
    let row_limit: Option<u64> = args.iter().position(|a| a == "--limit")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());
    if let Some(l) = row_limit {
        eprintln!("[extract] row limit: {}", l);
    }

    let headers = reader.headers()?.clone();
    let id_idx = headers.iter().position(|h| h == "id")
        .expect("no 'id' column in header");
    let cluster_idx = headers.iter().position(|h| h == "cluster_id")
        .expect("no 'cluster_id' column in header");
    eprintln!("[extract] id@{}  cluster_id@{}  (of {} columns)",
              id_idx, cluster_idx, headers.len());

    let out_file = File::create(output)?;
    let mut out = BufWriter::with_capacity(1 << 20, out_file);

    let t0 = Instant::now();
    let mut rows = 0u64;
    let mut kept = 0u64;
    for result in reader.records() {
        rows += 1;
        if let Some(l) = row_limit {
            if rows > l { break; }
        }
        let record = match result {
            Ok(r) => r,
            Err(_) => continue, // ragged row — skip
        };
        let id = match record.get(id_idx).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => continue,
        };
        let cluster_id = match record.get(cluster_idx).and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => continue,
        };
        if let Some(ref f) = filter {
            if !f.contains(&cluster_id) { continue; }
        }
        writeln!(out, "{},{}", id, cluster_id)?;
        kept += 1;

        if rows % 500_000 == 0 {
            let elapsed = t0.elapsed().as_secs_f64();
            eprintln!(
                "[extract] {:>10} rows  {:>10} kept  {:.0} rows/s",
                rows, kept, rows as f64 / elapsed,
            );
        }
    }

    let elapsed = t0.elapsed().as_secs_f64();
    eprintln!(
        "[extract] DONE: {} rows scanned, {} kept, {:.1}s ({:.0} rows/s)",
        rows, kept, elapsed, rows as f64 / elapsed,
    );

    Ok(())
}
