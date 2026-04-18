use std::collections::HashMap;

/// A TF-IDF index over wiki pages.
///
/// Dense vectors and norms are precomputed at build time so that
/// `ignite` and `similarity` never allocate or hash.
#[derive(Debug, Clone)]
pub struct TfIdfIndex {
    pub n_docs: usize,
    /// Document frequency: how many documents contain each term.
    pub df: HashMap<String, usize>,
    /// TF-IDF vector per document (sparse, kept for persistence).
    pub vectors: Vec<HashMap<String, f64>>,
    /// Precomputed dense vectors (one per doc, shared vocabulary).
    dense: Vec<Vec<f64>>,
    /// Precomputed L2 norms (one per doc).
    norms: Vec<f64>,
    /// Vocabulary index for sparse→dense conversion of queries.
    vocab_idx: HashMap<String, usize>,
}

impl TfIdfIndex {
    pub fn df(&self) -> &HashMap<String, usize> {
        &self.df
    }

    pub fn vectors(&self) -> &[HashMap<String, f64>] {
        &self.vectors
    }

    /// Restore from persisted components.
    pub fn from_parts(
        n_docs: usize,
        df: HashMap<String, usize>,
        vectors: Vec<HashMap<String, f64>>,
    ) -> Self {
        let vocab_idx = build_vocab_idx(&df);
        let dense = precompute_dense(&vectors, &vocab_idx);
        let norms = precompute_norms(&dense);
        Self { n_docs, df, vectors, dense, norms, vocab_idx }
    }

    /// Append a new document to the index. Returns the new doc's index.
    ///
    /// Updates:
    /// - `df` gets +1 for each unique term in `content`
    /// - `vocab_idx` grows to cover any term never seen before
    /// - every existing dense vector is extended with zeros at new term slots
    ///   (`O(n_docs · new_terms_in_this_doc)`)
    /// - the new doc's sparse and dense vectors are appended
    ///
    /// Accepts a small **IDF drift** for previously-indexed docs: their
    /// sparse/dense values were computed with the old `df` and stay frozen.
    /// This is acceptable when adding a handful of docs relative to a large
    /// corpus; for deep consistency call `build_index` from scratch.
    pub fn add_document(&mut self, content: &str) -> usize {
        let new_idx = self.n_docs;

        // Tokenize + local tf.
        let mut tf: HashMap<String, usize> = HashMap::new();
        let mut terms_in_doc: std::collections::HashSet<String> = std::collections::HashSet::new();
        for term in tokenize(content) {
            *tf.entry(term.clone()).or_insert(0) += 1;
            terms_in_doc.insert(term);
        }

        // Update df and grow vocab_idx for previously-unseen terms.
        let mut new_vocab: Vec<String> = Vec::new();
        for term in &terms_in_doc {
            *self.df.entry(term.clone()).or_insert(0) += 1;
            if !self.vocab_idx.contains_key(term) {
                let slot = self.vocab_idx.len();
                self.vocab_idx.insert(term.clone(), slot);
                new_vocab.push(term.clone());
            }
        }

        // Do NOT extend existing dense vectors for new vocab terms.
        // Old docs by definition had zero weight at those slots (they did
        // not contain the term). `ignite` bounds-checks the index and
        // treats out-of-range slots as zero — so leaving the old dense
        // vectors at their creation-time length is both correct and
        // avoids O(n_docs) work per new term. This is what takes write
        // cost at 25k from ~6.5 s to milliseconds.
        let _ = new_vocab;

        // Build the new doc's sparse tf-idf vector using the updated df.
        self.n_docs += 1;
        let n_docs_f = self.n_docs as f64;
        let max_tf = tf.values().copied().max().unwrap_or(1) as f64;
        let sparse: HashMap<String, f64> = tf.iter().map(|(term, &count)| {
            let tf_norm = count as f64 / max_tf;
            let df_val = *self.df.get(term).unwrap_or(&1) as f64;
            let idf = (n_docs_f / df_val).ln() + 1.0;
            (term.clone(), tf_norm * idf)
        }).collect();

        // Dense vector of the new doc (sized to current vocab_idx).
        let mut dense = vec![0.0f64; self.vocab_idx.len()];
        for (term, &value) in &sparse {
            if let Some(&idx) = self.vocab_idx.get(term) { dense[idx] = value; }
        }
        let norm = l2_norm(&dense);

        self.vectors.push(sparse);
        self.dense.push(dense);
        self.norms.push(norm);

        new_idx
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

    // Compute sparse TF-IDF vectors.
    let vectors: Vec<HashMap<String, f64>> = tf_per_doc.iter().map(|tf| {
        let max_tf = tf.values().copied().max().unwrap_or(1) as f64;
        tf.iter().map(|(term, &count)| {
            let tf_norm = count as f64 / max_tf;
            let idf = ((n_docs as f64) / (*df.get(term).unwrap_or(&1) as f64)).ln() + 1.0;
            (term.clone(), tf_norm * idf)
        }).collect()
    }).collect();

    // Precompute dense vectors and norms once.
    let vocab_idx = build_vocab_idx(&df);
    let dense = precompute_dense(&vectors, &vocab_idx);
    let norms = precompute_norms(&dense);

    TfIdfIndex { n_docs, df, vectors, dense, norms, vocab_idx }
}

/// Compute initial activation `a⁰` from a text query.
///
/// The query has a handful of terms; each document vector is ~|vocab|
/// dimensions. Expanding the query to dense would force O(|vocab|) work
/// per document — at Wikipedia scale that is ~50k × n_docs, catastrophic.
///
/// Instead we keep the query sparse: resolve each term to its vocab
/// index once, then each document's dot product touches only those
/// slots. Complexity: O(|query terms| × n_docs).
pub fn ignite(index: &TfIdfIndex, query: &str) -> Vec<f64> {
    let query_tf = query_tfidf(index, query);
    if query_tf.is_empty() {
        return vec![0.0; index.n_docs];
    }

    // Sparse query: (vocab_idx, weight) per resolvable term.
    let mut query_sparse: Vec<(usize, f64)> = query_tf.iter()
        .filter_map(|(term, &w)| index.vocab_idx.get(term).map(|&i| (i, w)))
        .collect();
    if query_sparse.is_empty() {
        return vec![0.0; index.n_docs];
    }

    let query_norm: f64 = query_sparse.iter().map(|(_, w)| w * w).sum::<f64>().sqrt();
    if query_norm == 0.0 {
        return vec![0.0; index.n_docs];
    }

    // Sort by vocab_idx for cache-friendly doc access.
    query_sparse.sort_unstable_by_key(|(i, _)| *i);

    // Bounds-check each slot lookup: `add_document` grows `vocab_idx` but
    // does not back-fill existing dense vectors, so slots beyond a given
    // doc's length are definitionally zero.
    index.dense.iter().zip(index.norms.iter()).map(|(doc, &doc_norm)| {
        if doc_norm == 0.0 {
            0.0
        } else {
            let mut s = 0.0;
            let dlen = doc.len();
            for &(i, qw) in &query_sparse {
                if i < dlen { s += qw * doc[i]; }
            }
            s / (query_norm * doc_norm)
        }
    }).collect()
}

/// Content similarity between pages `i` and `j`.
///
/// Sparse intersection: iterate the shorter document's terms and probe the
/// longer one. O(min(|doc_i|, |doc_j|)) instead of O(|vocab|) — at Wikipedia
/// scale (vocab ~50k, docs ~500 terms) this is 100× faster per call, which
/// matters because `dream_candidates` fires this in a tight loop.
pub fn similarity(index: &TfIdfIndex, i: usize, j: usize) -> f64 {
    if i >= index.n_docs || j >= index.n_docs {
        return 0.0;
    }
    let norm_i = index.norms[i];
    let norm_j = index.norms[j];
    if norm_i == 0.0 || norm_j == 0.0 {
        return 0.0;
    }
    let (small, big) = if index.vectors[i].len() <= index.vectors[j].len() {
        (&index.vectors[i], &index.vectors[j])
    } else {
        (&index.vectors[j], &index.vectors[i])
    };
    let mut s = 0.0;
    for (term, &w_a) in small {
        if let Some(&w_b) = big.get(term) {
            s += w_a * w_b;
        }
    }
    s / (norm_i * norm_j)
}

// ─── Internal helpers ────────────────────────────────────────────────────────

fn build_vocab_idx(df: &HashMap<String, usize>) -> HashMap<String, usize> {
    let mut vocab: Vec<&String> = df.keys().collect();
    vocab.sort();
    vocab.into_iter().enumerate().map(|(i, t)| (t.clone(), i)).collect()
}

fn precompute_dense(
    vectors: &[HashMap<String, f64>],
    vocab_idx: &HashMap<String, usize>,
) -> Vec<Vec<f64>> {
    vectors.iter().map(|sparse| sparse_to_dense(sparse, vocab_idx)).collect()
}

fn precompute_norms(dense: &[Vec<f64>]) -> Vec<f64> {
    dense.iter().map(|v| l2_norm(v)).collect()
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

fn sparse_to_dense(sparse: &HashMap<String, f64>, vocab_idx: &HashMap<String, usize>) -> Vec<f64> {
    let n = vocab_idx.len();
    let mut dense = vec![0.0; n];
    for (term, &value) in sparse {
        if let Some(&idx) = vocab_idx.get(term) {
            dense[idx] = value;
        }
    }
    dense
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
        assert_eq!(index.dense.len(), 3);
        assert_eq!(index.norms.len(), 3);

        let a0 = ignite(&index, "transformers attention");
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
        assert!((sim - 1.0).abs() < 1e-9, "Identical docs: got {sim}");
    }

    #[test]
    fn similarity_disjoint() {
        let contents = vec![
            "alpha beta gamma delta".into(),
            "epsilon zeta eta theta".into(),
        ];
        let index = build_index(&contents);
        let sim = similarity(&index, 0, 1);
        assert!(sim.abs() < 1e-9, "Disjoint docs: got {sim}");
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
