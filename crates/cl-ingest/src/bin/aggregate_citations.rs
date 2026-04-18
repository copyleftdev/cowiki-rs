//! Parallel citation-map aggregator. Walks the (uncompressed) CourtListener
//! citation-map CSV, maps opinion→cluster, keeps edges whose endpoints are
//! both in a filter set, and emits summed cluster→cluster depths.
//!
//! Design notes:
//! - Input is expected to be plain CSV (~3–5 GiB for the full map). If a
//!   `.bz2` is passed we decompress to a sibling `.csv` once, because bz2
//!   decode is single-threaded and the same bytes get walked on every rerun.
//! - File is `mmap`ed; line boundaries are found with SIMD memchr.
//! - Body is sharded across `rayon::current_num_threads()`, each worker
//!   parses its slice into a thread-local `ahash` HashMap, then we fold
//!   them into one. Integer parsing uses `atoi` (skips UTF-8 validation).
//! - Progress is flushed to stderr from each thread every ~1M rows so the
//!   operator can see throughput and notice stalls live.
//!
//! Usage:
//!   aggregate_citations <citation-map.csv | citation-map.csv.bz2> \
//!                       <opinion_to_cluster.csv> \
//!                       <cluster_filter.csv> \
//!                       <out_edges.csv>

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ahash::{AHashMap, AHashSet};
use bzip2::read::MultiBzDecoder;
use memmap2::Mmap;
use rayon::prelude::*;

fn load_u64_set(p: &Path) -> std::io::Result<AHashSet<u64>> {
    let f = File::open(p)?;
    let r = BufReader::with_capacity(1 << 20, f);
    let mut set = AHashSet::with_capacity(1 << 20);
    for line in r.lines() {
        let s = line?;
        if let Some(n) = atoi::atoi::<u64>(s.trim().as_bytes()) {
            set.insert(n);
        }
    }
    Ok(set)
}

fn load_opinion_to_cluster(p: &Path) -> std::io::Result<AHashMap<u64, u64>> {
    let f = File::open(p)?;
    let r = BufReader::with_capacity(1 << 20, f);
    let mut m: AHashMap<u64, u64> = AHashMap::with_capacity(16 << 20);
    for line in r.lines() {
        let s = line?;
        if let Some((a, b)) = s.split_once(',') {
            if let (Some(oid), Some(cid)) = (
                atoi::atoi::<u64>(a.as_bytes()),
                atoi::atoi::<u64>(b.as_bytes()),
            ) {
                m.insert(oid, cid);
            }
        }
    }
    Ok(m)
}

fn ensure_plain_csv(in_path: &Path) -> std::io::Result<PathBuf> {
    if in_path.extension().and_then(|s| s.to_str()) != Some("bz2") {
        return Ok(in_path.to_path_buf());
    }
    let sibling = in_path.with_extension(""); // strips trailing .bz2
    if sibling.exists() {
        return Ok(sibling);
    }
    eprintln!(
        "[aggr] decompressing {} → {} (one-time)",
        in_path.display(),
        sibling.display()
    );
    let t0 = Instant::now();
    let f = File::open(in_path)?;
    let mut bz = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, f));
    let mut out = BufWriter::with_capacity(1 << 20, File::create(&sibling)?);
    std::io::copy(&mut bz, &mut out)?;
    out.flush()?;
    eprintln!(
        "[aggr]   decompressed in {:.1}s",
        t0.elapsed().as_secs_f64()
    );
    Ok(sibling)
}

/// Trim CSV quoting from an all-digit field: `"123"` → `123`.
#[inline]
fn unquote(b: &[u8]) -> &[u8] {
    let start = if b.first() == Some(&b'"') { 1 } else { 0 };
    let end = if b.len() > start && b.last() == Some(&b'"') {
        b.len() - 1
    } else {
        b.len()
    };
    if end > start {
        &b[start..end]
    } else {
        &[]
    }
}

/// Parse a comma-separated line, extracting the fields at three known
/// column indices. Returns `None` if any is missing or malformed.
#[inline]
fn parse_triple(
    line: &[u8],
    a_idx: usize,
    b_idx: usize,
    c_idx: usize,
) -> Option<(u64, u64, u32)> {
    let mut a: Option<u64> = None;
    let mut b: Option<u64> = None;
    let mut c: Option<u32> = None;
    let mut col = 0usize;
    let mut field_start = 0usize;
    let mut i = 0usize;
    while i < line.len() {
        if line[i] == b',' {
            let field = unquote(&line[field_start..i]);
            if col == a_idx {
                a = atoi::atoi::<u64>(field);
            } else if col == b_idx {
                b = atoi::atoi::<u64>(field);
            } else if col == c_idx {
                c = atoi::atoi::<u32>(field);
            }
            field_start = i + 1;
            col += 1;
        }
        i += 1;
    }
    let field = unquote(&line[field_start..]);
    if col == a_idx {
        a = atoi::atoi::<u64>(field);
    } else if col == b_idx {
        b = atoi::atoi::<u64>(field);
    } else if col == c_idx {
        c = atoi::atoi::<u32>(field);
    }
    Some((a?, b?, c?))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!(
            "usage: {} <citation-map.csv[.bz2]> <opinion_to_cluster.csv> <cluster_filter.csv> <out_edges.csv>",
            args[0]
        );
        std::process::exit(2);
    }
    let in_path = Path::new(&args[1]);
    let o2c_path = Path::new(&args[2]);
    let filter_path = Path::new(&args[3]);
    let out_path = Path::new(&args[4]);

    eprintln!("[aggr] loading opinion→cluster from {}", o2c_path.display());
    let t0 = Instant::now();
    let o2c = load_opinion_to_cluster(o2c_path)?;
    eprintln!(
        "[aggr]   {} entries in {:.1}s",
        o2c.len(),
        t0.elapsed().as_secs_f64()
    );

    eprintln!("[aggr] loading cluster filter from {}", filter_path.display());
    let t1 = Instant::now();
    let filter = load_u64_set(filter_path)?;
    eprintln!(
        "[aggr]   {} cluster ids in {:.1}s",
        filter.len(),
        t1.elapsed().as_secs_f64()
    );

    let csv_path = ensure_plain_csv(in_path)?;
    eprintln!("[aggr] mmap {}", csv_path.display());
    let f = File::open(&csv_path)?;
    let mmap = unsafe { Mmap::map(&f)? };
    let bytes: &[u8] = &mmap;
    eprintln!("[aggr]   {} bytes ({:.2} GiB)", bytes.len(), bytes.len() as f64 / (1u64 << 30) as f64);

    // Header.
    let header_end = memchr::memchr(b'\n', bytes).ok_or("csv has no newline")?;
    let header_line = std::str::from_utf8(&bytes[..header_end])?
        .trim_end_matches('\r');
    let cols: Vec<&str> = header_line.split(',').collect();
    let citing_idx = cols
        .iter()
        .position(|&c| c == "citing_opinion_id")
        .ok_or("no citing_opinion_id column")?;
    let cited_idx = cols
        .iter()
        .position(|&c| c == "cited_opinion_id")
        .ok_or("no cited_opinion_id column")?;
    let depth_idx = cols
        .iter()
        .position(|&c| c == "depth")
        .ok_or("no depth column")?;
    eprintln!(
        "[aggr] columns: citing@{} cited@{} depth@{} ({} total)",
        citing_idx, cited_idx, depth_idx, cols.len()
    );

    let body = &bytes[header_end + 1..];

    // Split body into ~num_threads chunks on newline boundaries.
    let n_threads = rayon::current_num_threads();
    let chunk_size = body.len().div_ceil(n_threads);
    let mut boundaries: Vec<usize> = Vec::with_capacity(n_threads + 1);
    boundaries.push(0);
    let mut cursor = 0usize;
    for _ in 1..n_threads {
        if cursor >= body.len() {
            break;
        }
        let target = (cursor + chunk_size).min(body.len());
        let adjusted = if target >= body.len() {
            body.len()
        } else {
            match memchr::memchr(b'\n', &body[target..]) {
                Some(off) => target + off + 1,
                None => body.len(),
            }
        };
        boundaries.push(adjusted);
        cursor = adjusted;
    }
    boundaries.push(body.len());
    boundaries.dedup();
    let chunk_count = boundaries.len() - 1;
    eprintln!(
        "[aggr] parallel aggregation: {} chunks × {} rayon threads",
        chunk_count, n_threads
    );

    let rows_total = AtomicU64::new(0);
    let matched_total = AtomicU64::new(0);
    let t_agg = Instant::now();

    let ranges: Vec<(usize, usize)> = boundaries.windows(2).map(|w| (w[0], w[1])).collect();
    let local_maps: Vec<AHashMap<(u64, u64), u32>> = ranges
        .into_par_iter()
        .map(|(lo, hi)| {
            let slice = &body[lo..hi];
            let mut local: AHashMap<(u64, u64), u32> = AHashMap::with_capacity(1 << 16);
            let mut local_rows = 0u64;
            let mut local_matched = 0u64;
            let mut cur = 0usize;
            while cur < slice.len() {
                let end = memchr::memchr(b'\n', &slice[cur..])
                    .map(|e| cur + e)
                    .unwrap_or(slice.len());
                let line = &slice[cur..end];
                cur = end + 1;
                if line.is_empty() {
                    continue;
                }
                local_rows += 1;

                let (citing, cited, depth) = match parse_triple(line, citing_idx, cited_idx, depth_idx) {
                    Some(t) => t,
                    None => continue,
                };
                let cc = match o2c.get(&citing) {
                    Some(&v) => v,
                    None => continue,
                };
                let tc = match o2c.get(&cited) {
                    Some(&v) => v,
                    None => continue,
                };
                if cc == tc {
                    continue;
                }
                if !filter.contains(&cc) || !filter.contains(&tc) {
                    continue;
                }
                *local.entry((cc, tc)).or_insert(0) += depth;
                local_matched += 1;

                if local_rows % 1_000_000 == 0 {
                    let rows = rows_total.fetch_add(local_rows, Ordering::Relaxed) + local_rows;
                    let matched = matched_total.fetch_add(local_matched, Ordering::Relaxed)
                        + local_matched;
                    let el = t_agg.elapsed().as_secs_f64();
                    eprintln!(
                        "[aggr] {:>11} rows  {:>10} matched  {:.0} rows/s",
                        rows,
                        matched,
                        rows as f64 / el.max(1e-6)
                    );
                    local_rows = 0;
                    local_matched = 0;
                }
            }
            rows_total.fetch_add(local_rows, Ordering::Relaxed);
            matched_total.fetch_add(local_matched, Ordering::Relaxed);
            local
        })
        .collect();

    let agg_el = t_agg.elapsed().as_secs_f64();
    eprintln!(
        "[aggr] parallel scan done in {:.1}s; merging {} local maps",
        agg_el,
        local_maps.len()
    );

    let t_merge = Instant::now();
    let cap: usize = local_maps.iter().map(|m| m.len()).sum();
    let mut merged: AHashMap<(u64, u64), u32> = AHashMap::with_capacity(cap);
    for local in local_maps {
        for (k, v) in local {
            *merged.entry(k).or_insert(0) += v;
        }
    }
    eprintln!(
        "[aggr] merged {} unique edges in {:.1}s",
        merged.len(),
        t_merge.elapsed().as_secs_f64()
    );

    let rows = rows_total.load(Ordering::Relaxed);
    let matched = matched_total.load(Ordering::Relaxed);
    let total = t_agg.elapsed().as_secs_f64();
    eprintln!(
        "[aggr] DONE: {} rows, {} matched, {} unique cluster→cluster edges, {:.1}s ({:.0} rows/s)",
        rows,
        matched,
        merged.len(),
        total,
        rows as f64 / total.max(1e-6),
    );

    let out = File::create(out_path)?;
    let mut w = BufWriter::with_capacity(1 << 20, out);
    for ((src, dst), d) in &merged {
        writeln!(w, "{},{},{}", src, dst, d)?;
    }
    w.flush()?;
    eprintln!("[aggr] wrote {} edges to {}", merged.len(), out_path.display());

    Ok(())
}
