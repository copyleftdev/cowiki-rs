use std::fs;
use std::path::Path;

use crate::types::{PageId, WikiError};

/// Create a new markdown page on disk.
pub fn create_page(
    wiki_root: &Path,
    id: &PageId,
    title: &str,
    content: &str,
) -> Result<(), WikiError> {
    let path = wiki_root.join(format!("{}.md", id.0));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let full_content = format!("# {title}\n\n{content}\n");
    fs::write(&path, full_content)?;

    Ok(())
}

/// Update an existing page's content on disk.
pub fn update_page(
    wiki_root: &Path,
    id: &PageId,
    content: &str,
) -> Result<(), WikiError> {
    let path = wiki_root.join(format!("{}.md", id.0));

    if !path.exists() {
        return Err(WikiError::PageNotFound(id.clone()));
    }

    fs::write(&path, content)?;
    Ok(())
}

/// Append a backlink `[[target]]` to the source page.
///
/// If the page already contains a "## See Also" section, the link is
/// appended there. Otherwise, a new section is created at the end.
pub fn add_backlink(
    wiki_root: &Path,
    source: &PageId,
    target: &PageId,
) -> Result<(), WikiError> {
    let path = wiki_root.join(format!("{}.md", source.0));

    if !path.exists() {
        return Err(WikiError::PageNotFound(source.clone()));
    }

    let mut content = fs::read_to_string(&path)?;
    let link = format!("[[{}]]", target.0);

    // Don't add if already linked.
    if content.contains(&link) {
        return Ok(());
    }

    if let Some(pos) = content.find("## See Also") {
        // Find the end of the See Also section (next heading or EOF).
        let section_start = pos + "## See Also".len();
        let insert_pos = content[section_start..]
            .find("\n## ")
            .map(|p| section_start + p)
            .unwrap_or(content.len());
        content.insert_str(insert_pos, &format!("\n- {link}"));
    } else {
        content.push_str(&format!("\n\n## See Also\n\n- {link}\n"));
    }

    fs::write(&path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn create_and_read() {
        let tmp = TempDir::new().unwrap();
        let id = PageId("test-page".into());

        create_page(tmp.path(), &id, "Test Page", "Hello world.").unwrap();

        let content = fs::read_to_string(tmp.path().join("test-page.md")).unwrap();
        assert!(content.contains("# Test Page"));
        assert!(content.contains("Hello world."));
    }

    #[test]
    fn create_nested() {
        let tmp = TempDir::new().unwrap();
        let id = PageId("ai/deep/page".into());

        create_page(tmp.path(), &id, "Deep Page", "Content.").unwrap();
        assert!(tmp.path().join("ai/deep/page.md").exists());
    }

    #[test]
    fn update_existing() {
        let tmp = TempDir::new().unwrap();
        let id = PageId("page".into());

        create_page(tmp.path(), &id, "Page", "Old content.").unwrap();
        update_page(tmp.path(), &id, "# Page\n\nNew content.\n").unwrap();

        let content = fs::read_to_string(tmp.path().join("page.md")).unwrap();
        assert!(content.contains("New content."));
        assert!(!content.contains("Old content."));
    }

    #[test]
    fn update_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let id = PageId("nope".into());
        let result = update_page(tmp.path(), &id, "content");
        assert!(matches!(result, Err(WikiError::PageNotFound(_))));
    }

    #[test]
    fn add_backlink_creates_section() {
        let tmp = TempDir::new().unwrap();
        let src = PageId("source".into());
        let tgt = PageId("target".into());

        create_page(tmp.path(), &src, "Source", "Some content.").unwrap();
        add_backlink(tmp.path(), &src, &tgt).unwrap();

        let content = fs::read_to_string(tmp.path().join("source.md")).unwrap();
        assert!(content.contains("## See Also"));
        assert!(content.contains("[[target]]"));
    }

    #[test]
    fn add_backlink_no_duplicate() {
        let tmp = TempDir::new().unwrap();
        let src = PageId("source".into());
        let tgt = PageId("target".into());

        create_page(tmp.path(), &src, "Source", "Already has [[target]] link.").unwrap();
        add_backlink(tmp.path(), &src, &tgt).unwrap();

        let content = fs::read_to_string(tmp.path().join("source.md")).unwrap();
        // Should NOT have added a See Also section since link already exists.
        assert!(!content.contains("## See Also"));
    }
}
