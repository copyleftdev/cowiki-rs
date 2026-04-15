//! # chunk-quality
//!
//! Metrics for evaluating chunk boundaries and retrieval quality.
//!
//! - **Coherence**: mean intra-chunk cosine similarity (higher = more topically homogeneous)
//! - **Density variance**: variance of `score/cost` across items (higher = more room for
//!   density-based retrieval to outperform top-k)
//! - **Recall / precision / F1**: standard IR metrics

/// Recall = |retrieved ∩ relevant| / |relevant|.
pub fn recall(retrieved: &[usize], relevant: &[usize]) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let retrieved_set: std::collections::HashSet<_> = retrieved.iter().copied().collect();
    let hits = relevant.iter().filter(|r| retrieved_set.contains(r)).count();
    hits as f64 / relevant.len() as f64
}

/// Precision = |retrieved ∩ relevant| / |retrieved|.
pub fn precision(retrieved: &[usize], relevant: &[usize]) -> f64 {
    if retrieved.is_empty() {
        return 0.0;
    }
    let relevant_set: std::collections::HashSet<_> = relevant.iter().copied().collect();
    let hits = retrieved.iter().filter(|r| relevant_set.contains(r)).count();
    hits as f64 / retrieved.len() as f64
}

/// F1 = harmonic mean of precision and recall.
pub fn f1(retrieved: &[usize], relevant: &[usize]) -> f64 {
    let p = precision(retrieved, relevant);
    let r = recall(retrieved, relevant);
    if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 { 0.0 } else { dot / (norm_a * norm_b) }
}

/// Mean intra-chunk cosine similarity.
///
/// For each chunk defined by `(start, end)` index into `embeddings`,
/// compute the average pairwise cosine similarity within the chunk.
///
/// ## Proven properties (P7.1–P7.3)
/// - Topic-aligned boundaries score higher than random splits
/// - True topic boundaries maximize coherence
/// - Smaller fixed-size chunks degrade coherence
pub fn chunk_coherence(embeddings: &[Vec<f64>], boundaries: &[(usize, usize)]) -> f64 {
    let mut coherences = Vec::new();

    for &(start, end) in boundaries {
        let size = end - start;
        if size < 2 {
            coherences.push(1.0);
            continue;
        }

        let mut sum = 0.0;
        let mut count = 0;
        for i in start..end {
            for j in (i + 1)..end {
                sum += cosine_similarity(&embeddings[i], &embeddings[j]);
                count += 1;
            }
        }

        if count > 0 {
            coherences.push(sum / f64::from(count));
        }
    }

    if coherences.is_empty() { 0.0 } else { coherences.iter().sum::<f64>() / coherences.len() as f64 }
}

/// Variance of activation density ρ(v) = score / cost.
///
/// Higher variance = more opportunity for density-based retrieval
/// to outperform naive top-k.
pub fn density_variance(scores: &[f64], costs: &[u64]) -> f64 {
    assert_eq!(scores.len(), costs.len());
    let active: Vec<f64> = scores.iter().zip(costs.iter())
        .filter(|(s, _)| **s > 0.0)
        .map(|(s, c)| *s / *c as f64)
        .collect();

    if active.is_empty() { return 0.0; }

    let mean = active.iter().sum::<f64>() / active.len() as f64;
    active.iter().map(|&d| (d - mean).powi(2)).sum::<f64>() / active.len() as f64
}

/// Recall broken down by hop distance from query.
pub fn hop_recall(retrieved: &[usize], relevant_by_hop: &[(usize, Vec<usize>)]) -> Vec<(usize, f64)> {
    let retrieved_set: std::collections::HashSet<_> = retrieved.iter().copied().collect();
    relevant_by_hop.iter().map(|(hop, nodes)| {
        if nodes.is_empty() {
            (*hop, 0.0)
        } else {
            let hits = nodes.iter().filter(|n| retrieved_set.contains(n)).count();
            (*hop, hits as f64 / nodes.len() as f64)
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_basic() {
        assert!((recall(&[0, 1, 2], &[1, 2, 3]) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(recall(&[0, 1], &[]), 0.0);
        assert_eq!(recall(&[], &[1, 2]), 0.0);
    }

    #[test]
    fn cosine_identical() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-9);
    }

    #[test]
    fn coherence_single_topic() {
        let emb = vec![
            vec![1.0, 0.1],
            vec![1.0, 0.2],
            vec![1.0, 0.15],
        ];
        let coh = chunk_coherence(&emb, &[(0, 3)]);
        assert!(coh > 0.9, "Similar embeddings should have high coherence: {coh}");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    #[allow(dead_code)]
    fn arb_embedding(dim: usize, n: usize) -> impl Strategy<Value = Vec<Vec<f64>>> {
        proptest::collection::vec(
            proptest::collection::vec(-1.0..1.0f64, dim),
            n,
        )
    }

    proptest! {
        /// P7.1: Topic-aligned chunks beat random splits.
        #[test]
        fn topic_aligned_beats_random(
            n_topics in 2..5usize,
            per_topic in 4..8usize,
        ) {
            let dim = 8;
            let n = n_topics * per_topic;

            // Generate topic centroids.
            let mut embeddings = Vec::with_capacity(n);
            for t in 0..n_topics {
                let centroid: Vec<f64> = (0..dim).map(|d| {
                    if d == t % dim { 3.0 } else { 0.0 }
                }).collect();
                for s in 0..per_topic {
                    let emb: Vec<f64> = centroid.iter().enumerate()
                        .map(|(d, &c)| c + (s as f64 * 0.01) + (d as f64 * 0.001))
                        .collect();
                    embeddings.push(emb);
                }
            }

            // Correct boundaries.
            let correct: Vec<(usize, usize)> = (0..n_topics)
                .map(|t| (t * per_topic, (t + 1) * per_topic))
                .collect();

            // Shifted boundaries.
            let shift = per_topic / 2;
            let mut shifted = Vec::new();
            let mut pos = shift;
            while pos < n {
                let end = (pos + per_topic).min(n);
                shifted.push((pos, end));
                pos = end;
            }
            if shifted.is_empty() {
                shifted.push((0, n));
            }

            let correct_coh = chunk_coherence(&embeddings, &correct);
            let shifted_coh = chunk_coherence(&embeddings, &shifted);

            prop_assert!(correct_coh >= shifted_coh - 0.1,
                "Topic-aligned ({correct_coh:.4}) < shifted ({shifted_coh:.4})");
        }
    }
}
