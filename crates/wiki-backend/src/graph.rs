use std::collections::HashMap;

use scored_graph::ScoredGraph;

use crate::types::PageMeta;

/// Build a `ScoredGraph` from page metadata and the index mapping.
///
/// Edges are created from wiki backlinks: if page `i` contains `[[target]]`
/// and target resolves to page `j`, then edge `i -> j` with weight 1.0.
///
/// The `ScoredGraph` constructor handles row-stochastic normalization,
/// so pages with many outgoing links distribute activation equally.
/// Dangling links (to non-existent pages) are silently skipped.
pub fn build_graph(pages: &[PageMeta], id_to_idx: &HashMap<String, usize>) -> ScoredGraph {
    let n = pages.len();
    if n == 0 {
        return ScoredGraph::new(0, vec![], vec![]);
    }

    let mut weights = vec![0.0; n * n];

    for (i, page) in pages.iter().enumerate() {
        for link in &page.links_to {
            if let Some(&j) = id_to_idx.get(&link.0)
                && i != j
            {
                weights[i * n + j] = 1.0;
            }
            // Dangling links silently skipped.
        }
    }

    let costs: Vec<u64> = pages.iter().map(|p| p.token_cost).collect();

    ScoredGraph::new(n, weights, costs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageId;
    use std::path::PathBuf;

    fn make_page(id: &str, links: &[&str]) -> PageMeta {
        PageMeta {
            id: PageId(id.into()),
            path: PathBuf::from(format!("{id}.md")),
            title: id.into(),
            links_to: links.iter().map(|l| PageId((*l).into())).collect(),
            token_cost: 100,
            category: 0,
        }
    }

    #[test]
    fn simple_graph() {
        let pages = vec![
            make_page("a", &["b"]),
            make_page("b", &["c"]),
            make_page("c", &[]),
        ];
        let idx: HashMap<String, usize> = [("a", 0), ("b", 1), ("c", 2)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let g = build_graph(&pages, &idx);

        assert_eq!(g.len(), 3);
        assert!(g.is_row_stochastic());
        assert!(g.raw_weight(0, 1) > 0.0); // a -> b
        assert!(g.raw_weight(1, 2) > 0.0); // b -> c
        assert_eq!(g.raw_weight(2, 0), 0.0); // c has no links
    }

    #[test]
    fn dangling_links_skipped() {
        let pages = vec![
            make_page("a", &["b", "nonexistent"]),
            make_page("b", &[]),
        ];
        let idx: HashMap<String, usize> = [("a", 0), ("b", 1)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();

        let g = build_graph(&pages, &idx);

        assert_eq!(g.len(), 2);
        assert!(g.is_row_stochastic());
        assert!(g.raw_weight(0, 1) > 0.0);
    }

    #[test]
    fn no_self_loops() {
        let pages = vec![make_page("a", &["a"])]; // self-link
        let idx: HashMap<String, usize> =
            [("a".to_string(), 0)].into_iter().collect();

        let g = build_graph(&pages, &idx);
        assert_eq!(g.raw_weight(0, 0), 0.0);
    }

    #[test]
    fn empty_wiki() {
        let g = build_graph(&[], &HashMap::new());
        assert_eq!(g.len(), 0);
    }
}
