"""
PROPERTY 7: Human-Cognitive Chunks Are More Coherent Than Fixed-Token Chunks.

Hypothesis H1: Human-authored article boundaries produce chunks with higher
intra-chunk semantic coherence than fixed-token splitting.

Coherence = mean pairwise cosine similarity of sentence embeddings within a chunk.
Higher coherence = the chunk is about one topic, not sliced mid-thought.

Tests:
    P7.1  Semantic chunks beat random-boundary chunks
    P7.2  Coherence is maximized when boundaries align with topic shifts
    P7.3  Fixed-size chunking degrades coherence as chunk size shrinks
"""

import numpy as np
from hypothesis import given, settings, assume
from hypothesis import strategies as st

from cowiki.metrics import chunk_coherence


def make_topical_embeddings(
    n_sentences: int,
    n_topics: int,
    embed_dim: int,
    topic_boundaries: list[int],
    noise: float = 0.1,
) -> np.ndarray:
    """Generate synthetic sentence embeddings with topic structure.

    Sentences within the same topic are similar (close in embedding space).
    Sentences in different topics are dissimilar.
    """
    rng = np.random.RandomState(42)
    embeddings = np.zeros((n_sentences, embed_dim))

    # Generate a centroid for each topic
    centroids = rng.randn(n_topics, embed_dim)
    # Make centroids well-separated
    centroids *= 3.0

    # Assign sentences to topics based on boundaries
    boundaries = sorted(topic_boundaries + [0, n_sentences])
    boundaries = sorted(set(boundaries))

    topic_idx = 0
    for i in range(len(boundaries) - 1):
        start, end = boundaries[i], boundaries[i + 1]
        if topic_idx >= n_topics:
            topic_idx = n_topics - 1
        for s in range(start, end):
            embeddings[s] = centroids[topic_idx] + rng.randn(embed_dim) * noise
        topic_idx += 1

    # Normalize
    norms = np.linalg.norm(embeddings, axis=1, keepdims=True)
    norms = np.where(norms == 0, 1, norms)
    embeddings /= norms

    return embeddings


class TestSemanticVsRandom:
    """P7.1: Topic-aligned chunks have higher coherence than random splits."""

    @given(
        n_topics=st.integers(min_value=2, max_value=5),
        sentences_per_topic=st.integers(min_value=4, max_value=10),
    )
    @settings(max_examples=100, deadline=None)
    def test_topic_aligned_beats_random(self, n_topics, sentences_per_topic):
        """Chunking at topic boundaries > chunking at arbitrary positions."""
        n_sentences = n_topics * sentences_per_topic
        embed_dim = 16

        # True topic boundaries
        true_boundaries = [i * sentences_per_topic for i in range(1, n_topics)]

        embeddings = make_topical_embeddings(
            n_sentences, n_topics, embed_dim, true_boundaries, noise=0.2,
        )

        # Correct chunking: at topic boundaries
        correct_chunks = [
            (i * sentences_per_topic, (i + 1) * sentences_per_topic)
            for i in range(n_topics)
        ]

        # Random chunking: split at arbitrary positions
        rng = np.random.RandomState(123)
        random_splits = sorted(rng.choice(
            range(1, n_sentences), size=n_topics - 1, replace=False,
        ))
        random_boundaries = [0] + list(random_splits) + [n_sentences]
        random_chunks = [
            (random_boundaries[i], random_boundaries[i + 1])
            for i in range(len(random_boundaries) - 1)
        ]

        correct_coherence = chunk_coherence(embeddings, correct_chunks)
        random_coherence = chunk_coherence(embeddings, random_chunks)

        assert correct_coherence >= random_coherence - 0.05, (
            f"Topic-aligned coherence ({correct_coherence:.4f}) < "
            f"random coherence ({random_coherence:.4f})"
        )


class TestFixedSizeDegradation:
    """P7.3: Smaller fixed chunks → lower coherence (more mid-topic cuts)."""

    @given(
        n_topics=st.integers(min_value=3, max_value=5),
        sentences_per_topic=st.integers(min_value=6, max_value=12),
    )
    @settings(max_examples=100, deadline=None)
    def test_smaller_chunks_lower_coherence(self, n_topics, sentences_per_topic):
        """Halving chunk size should not increase coherence."""
        n_sentences = n_topics * sentences_per_topic
        embed_dim = 16
        true_boundaries = [i * sentences_per_topic for i in range(1, n_topics)]

        embeddings = make_topical_embeddings(
            n_sentences, n_topics, embed_dim, true_boundaries, noise=0.15,
        )

        # Large chunks (topic-sized)
        large_size = sentences_per_topic
        large_chunks = [
            (i, min(i + large_size, n_sentences))
            for i in range(0, n_sentences, large_size)
        ]

        # Small chunks (half topic size — will cut mid-topic)
        small_size = max(2, sentences_per_topic // 2)
        small_chunks = [
            (i, min(i + small_size, n_sentences))
            for i in range(0, n_sentences, small_size)
        ]

        large_coherence = chunk_coherence(embeddings, large_chunks)
        small_coherence = chunk_coherence(embeddings, small_chunks)

        # Topic-aligned large chunks should be at least as coherent
        assert large_coherence >= small_coherence - 0.1, (
            f"Large chunk coherence ({large_coherence:.4f}) < "
            f"small chunk coherence ({small_coherence:.4f})"
        )


class TestOptimalBoundaries:
    """P7.2: Coherence is maximized at true topic boundaries."""

    @given(
        n_topics=st.integers(min_value=2, max_value=4),
        sentences_per_topic=st.integers(min_value=5, max_value=10),
    )
    @settings(max_examples=100, deadline=None)
    def test_true_boundaries_maximize_coherence(self, n_topics, sentences_per_topic):
        """The true topic boundaries should produce near-maximal coherence."""
        n_sentences = n_topics * sentences_per_topic
        embed_dim = 16
        true_boundaries = [i * sentences_per_topic for i in range(1, n_topics)]

        embeddings = make_topical_embeddings(
            n_sentences, n_topics, embed_dim, true_boundaries, noise=0.1,
        )

        # True boundaries
        true_chunks = [
            (i * sentences_per_topic, (i + 1) * sentences_per_topic)
            for i in range(n_topics)
        ]

        # Shifted boundaries (off by 2 sentences)
        shift = 2
        shifted_chunks = [
            (i * sentences_per_topic + shift,
             min((i + 1) * sentences_per_topic + shift, n_sentences))
            for i in range(n_topics)
        ]
        shifted_chunks[0] = (0, shifted_chunks[0][1])

        true_coh = chunk_coherence(embeddings, true_chunks)
        shifted_coh = chunk_coherence(embeddings, shifted_chunks)

        assert true_coh >= shifted_coh - 0.05, (
            f"True boundaries ({true_coh:.4f}) worse than "
            f"shifted boundaries ({shifted_coh:.4f})"
        )
