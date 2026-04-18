//! Dump a few opinion rows with plain_text + html_with_citations, for
//! sanity-checking the opinion-enrichment pipeline before we commit to
//! streaming all 54 GiB.

use std::fs::File;
use std::io::BufReader;

use bzip2::read::MultiBzDecoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "wiki-corpus/courtlistener-raw/opinions.csv.bz2".to_string());
    let n: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3);

    let f = File::open(&path)?;
    let bz = MultiBzDecoder::new(BufReader::with_capacity(1 << 20, f));
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .escape(Some(b'\\'))
        .double_quote(false)
        .from_reader(bz);

    let hdr = reader.headers()?.clone();
    let idx = |name: &str| hdr.iter().position(|h| h == name).unwrap_or(usize::MAX);
    let i_id = idx("id");
    let i_cluster = idx("cluster_id");
    let i_type = idx("type");
    let i_pt = idx("plain_text");
    let i_hw = idx("html_with_citations");

    let mut printed = 0usize;
    for (row_idx, r) in reader.records().enumerate() {
        let row = match r {
            Ok(r) => r,
            Err(e) => {
                eprintln!("row {}: parse error {e}", row_idx);
                continue;
            }
        };
        let pt = row.get(i_pt).unwrap_or("");
        let hw = row.get(i_hw).unwrap_or("");
        // Only show opinions that have body text — most useful sample.
        if pt.len() < 200 && hw.len() < 200 {
            continue;
        }
        println!("=== opinion {} cluster {} type={} ===",
            row.get(i_id).unwrap_or(""),
            row.get(i_cluster).unwrap_or(""),
            row.get(i_type).unwrap_or(""));
        println!("plain_text: {} chars", pt.len());
        println!("{}\n", &pt.chars().take(500).collect::<String>());
        println!("html_with_citations: {} chars", hw.len());
        println!("{}\n", &hw.chars().take(600).collect::<String>());
        println!("---");
        printed += 1;
        if printed >= n { break; }
    }

    Ok(())
}
