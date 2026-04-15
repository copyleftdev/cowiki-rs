use std::collections::HashMap;

/// A TF-IDF index over wiki pages.
#[derive(Debug, Clone)]
pub struct TfIdfIndex {
    /// Number of documents.
    pub n_docs: usize,
    /// Document frequency: how many documents contain each term.
    pub df: HashMap<String, usize>,
    /// TF-IDF vector per document (sparse).
    pub vectors: Vec<HashMap<String, f64>>,
    vocab_idx: HashMap<String, usize>,
}

impl TfIdfIndex {
    /// Serializable components for persistence.
    pub fn df(&self) -> &HashMap<String, usize> {
        &self.df
    }

    pub fn vectors(&self) -> &Vec<HashMap<String, f64>> {
        &self.vectors
    }

    /// Restore from persisted components.
    pub fn from_parts(
        n_docs: usize,
        df: HashMap<String, usize>,
        vectors: Vec<HashMap<String, f64>>,
    ) -> Self {
        let vocab: Vec<String> = {
            let mut v: Vec<String> = df.keys().cloned().collect();
            v.sort();
            v
        };
        let vocab_idx: HashMap<String, usize> = vocab.iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
        Self { n_docs, df, vectors, vocab_idx }
    }
}

/// Build a TF-IDF index from page contents.
pub fn build_index(contents: &[String]) -> TfIdfIndex {
    let n_docs = contents.len();
    let mut df: HashMap<String, usize> = HashMap::new();
    let mut tf_per_doc: Vec<HashMap<String, usize>> = Vec::with_capacity(n_docs);

    for content in contents {
        let mut tf: HashMap<String, usize> = HashMap::new();
        let mut seen_terms: std::collections::HashSet<String> = std::collections::HashSet::new();

        for term in tokenize(content) {
            *tf.entry(term.clone()).or_insert(0) += 1;
            if seen_terms.insert(term.clone()) {
                *df.entry(term).or_insert(0) += 1;
            }
        }
        tf_per_doc.push(tf);
    }

    // Compute TF-IDF vectors.
    let vectors: Vec<HashMap<String, f64>> = tf_per_doc.iter().map(|tf| {
        let max_tf = tf.values().copied().max().unwrap_or(1) as f64;
        tf.iter().map(|(term, &count)| {
            let tf_norm = count as f64 / max_tf;
            let idf = ((n_docs as f64) / (*df.get(term).unwrap_or(&1) as f64)).ln() + 1.0;
            (term.clone(), tf_norm * idf)
        }).collect()
    }).collect();

    // Build vocabulary.
    let mut vocab: Vec<String> = df.keys().cloned().collect();
    vocab.sort();
    let vocab_idx: HashMap<String, usize> = vocab.iter()
        .enumerate()
        .map(|(i, t)| (t.clone(), i))
        .collect();

    TfIdfIndex { n_docs, df, vectors, vocab_idx }
}

/// Compute initial activation `a⁰` from a text query.
///
/// Returns a `Vec<f64>` of length `n` where each entry is the cosine
/// similarity between the query's TF-IDF vector and the page's.
pub fn ignite(index: &TfIdfIndex, query: &str) -> Vec<f64> {
    let query_tf = query_tfidf(index, query);
    let query_dense = to_dense(&query_tf, &index.vocab_idx);
    let query_norm = l2_norm(&query_dense);

    if query_norm == 0.0 {
        return vec![0.0; index.n_docs];
    }

    index.vectors.iter().map(|doc_sparse| {
        let doc_dense = to_dense(doc_sparse, &index.vocab_idx);
        let doc_norm = l2_norm(&doc_dense);
        if doc_norm == 0.0 {
            0.0
        } else {
            dot(&query_dense, &doc_dense) / (query_norm * doc_norm)
        }
    }).collect()
}

/// Content similarity oracle for the dream operator.
///
/// Returns the cosine similarity between pages `i` and `j`.
pub fn similarity(index: &TfIdfIndex, i: usize, j: usize) -> f64 {
    if i >= index.vectors.len() || j >= index.vectors.len() {
        return 0.0;
    }
    let a = to_dense(&index.vectors[i], &index.vocab_idx);
    let b = to_dense(&index.vectors[j], &index.vocab_idx);
    let norm_a = l2_norm(&a);
    let norm_b = l2_norm(&b);
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot(&a, &b) / (norm_a * norm_b)
    }
}

fn query_tfidf(index: &TfIdfIndex, query: &str) -> HashMap<String, f64> {
    let mut tf: HashMap<String, usize> = HashMap::new();
    for term in tokenize(query) {
        *tf.entry(term).or_insert(0) += 1;
    }
    let max_tf = tf.values().copied().max().unwrap_or(1) as f64;

    tf.iter()
        .filter(|(term, _)| index.df.contains_key(*term))
        .map(|(term, &count)| {
            let tf_norm = count as f64 / max_tf;
            let idf = ((index.n_docs as f64) / (*index.df.get(term).unwrap_or(&1) as f64)).ln() + 1.0;
            (term.clone(), tf_norm * idf)
        })
        .collect()
}

fn to_dense(sparse: &HashMap<String, f64>, vocab_idx: &HashMap<String, usize>) -> Vec<f64> {
    let n = vocab_idx.len();
    let mut dense = vec![0.0; n];
    for (term, &value) in sparse {
        if let Some(&idx) = vocab_idx.get(term) {
            dense[idx] = value;
        }
    }
    dense
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 1)
        .map(|s| s.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_and_ignite() {
        let contents = vec![
            "Transformers are a neural network architecture using attention.".into(),
            "Random forests are an ensemble learning method.".into(),
            "Attention mechanisms are central to transformers.".into(),
        ];

        let index = build_index(&contents);
        assert_eq!(index.n_docs, 3);

        let a0 = ignite(&index, "transformers attention");

        // Pages 0 and 2 mention both terms, page 1 does not.
        assert!(a0[0] > a0[1], "Transformers page should rank above forests");
        assert!(a0[2] > a0[1], "Attention page should rank above forests");
    }

    #[test]
    fn similarity_identical() {
        let contents = vec![
            "the cat sat on the mat".into(),
            "the cat sat on the mat".into(),
        ];
        let index = build_index(&contents);
        let sim = similarity(&index, 0, 1);
        assert!((sim - 1.0).abs() < 1e-9, "Identical docs should have similarity 1.0, got {sim}");
    }

    #[test]
    fn similarity_disjoint() {
        let contents = vec![
            "alpha beta gamma delta".into(),
            "epsilon zeta eta theta".into(),
        ];
        let index = build_index(&contents);
        let sim = similarity(&index, 0, 1);
        assert!(sim.abs() < 1e-9, "Disjoint docs should have similarity ~0, got {sim}");
    }

    #[test]
    fn empty_query_zero_activation() {
        let contents = vec!["some content here".into()];
        let index = build_index(&contents);
        let a0 = ignite(&index, "");
        assert_eq!(a0[0], 0.0);
    }

    #[test]
    fn unknown_terms_zero_activation() {
        let contents = vec!["transformers and attention".into()];
        let index = build_index(&contents);
        let a0 = ignite(&index, "xylophone glockenspiel");
        assert_eq!(a0[0], 0.0);
    }
}
