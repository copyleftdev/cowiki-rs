use regex::Regex;
use std::sync::LazyLock;

use crate::types::PageId;

static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap());

static CODE_FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```.*?```").unwrap());

/// Extract wiki-style backlinks from markdown content.
///
/// Parses `[[target]]` and `[[target|display text]]` patterns.
/// Ignores links inside fenced code blocks.
/// Returns deduplicated, normalized `PageId` values.
pub fn extract_links(content: &str) -> Vec<PageId> {
    // Strip code blocks first so we don't parse links inside them.
    let stripped = CODE_FENCE.replace_all(content, "");

    let mut seen = std::collections::HashSet::new();
    let mut links = Vec::new();

    for cap in LINK_RE.captures_iter(&stripped) {
        let raw = cap[1].trim();
        let normalized = normalize_link(raw);
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            links.push(PageId(normalized));
        }
    }

    links
}

/// Extract the title from markdown content.
/// Returns the first `# Heading` line, or `None`.
pub fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

fn normalize_link(raw: &str) -> String {
    raw.to_lowercase()
        .replace(' ', "-")
        .replace('\\', "/")
        .trim_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_link() {
        let links = extract_links("See [[Transformers]] for details.");
        assert_eq!(links, vec![PageId("transformers".into())]);
    }

    #[test]
    fn link_with_display_text() {
        let links = extract_links("Read [[ai/models|the models page]].");
        assert_eq!(links, vec![PageId("ai/models".into())]);
    }

    #[test]
    fn multiple_links_deduplicated() {
        let links = extract_links("[[A]] and [[B]] and [[A]] again.");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0], PageId("a".into()));
        assert_eq!(links[1], PageId("b".into()));
    }

    #[test]
    fn links_inside_code_blocks_ignored() {
        let content = "Real [[link]] here.\n```\n[[fake]] link\n```\nEnd.";
        let links = extract_links(content);
        assert_eq!(links, vec![PageId("link".into())]);
    }

    #[test]
    fn no_links() {
        let links = extract_links("Just plain text, no links.");
        assert!(links.is_empty());
    }

    #[test]
    fn nested_directories() {
        let links = extract_links("See [[ai/deep-learning/transformers]].");
        assert_eq!(links, vec![PageId("ai/deep-learning/transformers".into())]);
    }

    #[test]
    fn extract_title_basic() {
        assert_eq!(
            extract_title("# My Page\n\nContent here."),
            Some("My Page".into()),
        );
    }

    #[test]
    fn extract_title_none() {
        assert_eq!(extract_title("No heading here."), None);
    }
}
