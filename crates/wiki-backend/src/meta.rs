//! Companion `.meta` files for human-readable page metadata.
//!
//! Each `page.md` gets a `page.meta` alongside it containing:
//! - title
//! - backlinks (outgoing)
//! - token cost
//! - category
//! - last modified timestamp
//!
//! These are the legible layer. You can `cat` them, `grep` them, diff them.
//! The engine reads them on cold start; SQLite is the hot cache.

use std::fs;
use std::path::Path;

use crate::types::{PageMeta, WikiError};

/// Write a companion `.meta` file for a page.
pub fn write_meta(page: &PageMeta) -> Result<(), WikiError> {
    let meta_path = page.path.with_extension("meta");

    let links: String = if page.links_to.is_empty() {
        "  (none)".to_string()
    } else {
        page.links_to.iter()
            .map(|l| format!("  - {}", l.0))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let content = format!(
        "title: {title}\n\
         tokens: {tokens}\n\
         category: {category}\n\
         links:\n\
         {links}\n",
        title = page.title,
        tokens = page.token_cost,
        category = page.category,
        links = links,
    );

    fs::write(meta_path, content)?;
    Ok(())
}

/// Write `.meta` files for all pages.
pub fn write_all_meta(pages: &[PageMeta]) -> Result<(), WikiError> {
    for page in pages {
        write_meta(page)?;
    }
    Ok(())
}

/// Read a `.meta` file back. Returns the raw text content.
/// Parsing is minimal since the scanner is the source of truth.
pub fn read_meta(md_path: &Path) -> Result<Option<String>, WikiError> {
    let meta_path = md_path.with_extension("meta");
    if meta_path.exists() {
        Ok(Some(fs::read_to_string(meta_path)?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageId;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_meta() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("test.md");
        fs::write(&md_path, "# Test").unwrap();

        let page = PageMeta {
            id: PageId("test".into()),
            path: md_path.clone(),
            title: "Test Page".into(),
            links_to: vec![PageId("other".into()), PageId("another".into())],
            token_cost: 42,
            category: 1,
        };

        write_meta(&page).unwrap();

        let meta = read_meta(&md_path).unwrap().unwrap();
        assert!(meta.contains("title: Test Page"));
        assert!(meta.contains("tokens: 42"));
        assert!(meta.contains("- other"));
        assert!(meta.contains("- another"));
    }

    #[test]
    fn meta_no_links() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("lonely.md");
        fs::write(&md_path, "# Lonely").unwrap();

        let page = PageMeta {
            id: PageId("lonely".into()),
            path: md_path.clone(),
            title: "Lonely".into(),
            links_to: vec![],
            token_cost: 10,
            category: 0,
        };

        write_meta(&page).unwrap();

        let meta = read_meta(&md_path).unwrap().unwrap();
        assert!(meta.contains("(none)"));
    }

    #[test]
    fn read_missing_meta() {
        let tmp = TempDir::new().unwrap();
        let md_path = tmp.path().join("no-meta.md");
        let result = read_meta(&md_path).unwrap();
        assert!(result.is_none());
    }
}
