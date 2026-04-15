use std::collections::HashMap;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::parse;
use crate::types::{PageId, PageMeta, WikiError};

/// Scan a directory tree for markdown files and build page metadata.
///
/// Skips the `.cowiki/` metadata directory.
/// Returns pages sorted by `PageId` for deterministic ordering.
pub fn scan_directory(root: &Path) -> Result<Vec<PageMeta>, WikiError> {
    let root = root.canonicalize()?;
    let mut pages = Vec::new();

    // Map directory names to category bit positions.
    let mut category_map: HashMap<String, u64> = HashMap::new();
    let mut next_category_bit: u64 = 0;

    for entry in WalkDir::new(&root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip the metadata directory.
        if path.components().any(|c| c.as_os_str() == ".cowiki") {
            continue;
        }

        // Only process .md files.
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let rel_path = path.strip_prefix(&root).unwrap_or(path);
        let page_id = path_to_page_id(rel_path);

        let content = fs::read_to_string(path)?;

        let title = parse::extract_title(&content)
            .unwrap_or_else(|| {
                rel_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
                    .to_string()
            });

        let links_to = parse::extract_links(&content);

        // Token cost: ~4 bytes per token, minimum 1.
        let token_cost = (content.len() as u64 / 4).max(1);

        // Category from parent directory.
        let category = if let Some(parent) = rel_path.parent() {
            let dir_name = parent.to_string_lossy().to_string();
            if dir_name.is_empty() || dir_name == "." {
                0
            } else {
                let bit = category_map.entry(dir_name).or_insert_with(|| {
                    let b = next_category_bit;
                    next_category_bit += 1;
                    b
                });
                1u64 << (*bit).min(63)
            }
        } else {
            0
        };

        pages.push(PageMeta {
            id: page_id,
            path: path.to_path_buf(),
            title,
            links_to,
            token_cost,
            category,
        });
    }

    // Sort for deterministic node ordering.
    pages.sort_by(|a, b| a.id.0.cmp(&b.id.0));

    Ok(pages)
}

/// Build the `PageId -> node index` mapping from a sorted page list.
pub fn build_index_map(pages: &[PageMeta]) -> HashMap<String, usize> {
    pages.iter()
        .enumerate()
        .map(|(i, p)| (p.id.0.clone(), i))
        .collect()
}

fn path_to_page_id(rel_path: &Path) -> PageId {
    let s = rel_path
        .with_extension("")
        .to_string_lossy()
        .to_string()
        .replace('\\', "/")
        .to_lowercase();
    PageId(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_page(dir: &Path, rel_path: &str, content: &str) {
        let path = dir.join(rel_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn scan_simple_wiki() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_page(root, "index.md", "# Home\n\nWelcome. See [[about]].");
        write_page(root, "about.md", "# About\n\nThis is the wiki.");

        let pages = scan_directory(root).unwrap();
        assert_eq!(pages.len(), 2);

        let home = pages.iter().find(|p| p.id.0 == "index").unwrap();
        assert_eq!(home.title, "Home");
        assert_eq!(home.links_to, vec![PageId("about".into())]);
        assert!(home.token_cost > 0);
    }

    #[test]
    fn scan_nested_directories() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_page(root, "ai/transformers.md", "# Transformers\n\nSee [[ai/attention]].");
        write_page(root, "ai/attention.md", "# Attention\n\nCore mechanism.");

        let pages = scan_directory(root).unwrap();
        assert_eq!(pages.len(), 2);

        let transformers = pages.iter().find(|p| p.id.0 == "ai/transformers").unwrap();
        assert_eq!(transformers.links_to, vec![PageId("ai/attention".into())]);
        // Both in same directory, same category.
        assert!(transformers.category > 0);
    }

    #[test]
    fn scan_skips_cowiki_dir() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_page(root, "page.md", "# Page");
        write_page(root, ".cowiki/index.json", "{}");

        let pages = scan_directory(root).unwrap();
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn scan_skips_non_md() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_page(root, "page.md", "# Page");
        write_page(root, "image.png", "not markdown");
        write_page(root, "notes.txt", "not markdown");

        let pages = scan_directory(root).unwrap();
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn deterministic_ordering() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_page(root, "zebra.md", "# Zebra");
        write_page(root, "alpha.md", "# Alpha");
        write_page(root, "middle.md", "# Middle");

        let pages = scan_directory(root).unwrap();
        let ids: Vec<&str> = pages.iter().map(|p| p.id.0.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "middle", "zebra"]);
    }
}
