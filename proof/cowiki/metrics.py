"""
Metrics for evaluating Co-Wiki hypotheses.

- Recall at budget B
- Chunk coherence (topic homogeneity)
- Activation density advantage
"""

from __future__ import annotations

import numpy as np
from numpy.typing import NDArray


def recall_at_budget(
    retrieved: list[int],
    relevant: set[int],
) -> float:
    """Recall = |retrieved ∩ relevant| / |relevant|.

    Returns 0.0 if relevant is empty (vacuously — nothing to find).
    """
    if not relevant:
        return 0.0
    hits = len(set(retrieved) & relevant)
    return hits / len(relevant)


def precision_at_budget(
    retrieved: list[int],
    relevant: set[int],
) -> float:
    """Precision = |retrieved ∩ relevant| / |retrieved|."""
    if not retrieved:
        return 0.0
    hits = len(set(retrieved) & relevant)
    return hits / len(retrieved)


def f1_score(
    retrieved: list[int],
    relevant: set[int],
) -> float:
    """Harmonic mean of precision and recall."""
    p = precision_at_budget(retrieved, relevant)
    r = recall_at_budget(retrieved, relevant)
    if p + r == 0:
        return 0.0
    return 2 * p * r / (p + r)


def chunk_coherence(
    embeddings: NDArray[np.float64],
    chunk_boundaries: list[tuple[int, int]],
) -> float:
    """Mean intra-chunk cosine similarity.

    For each chunk defined by (start, end) index into embeddings,
    compute the average pairwise cosine similarity within the chunk.

    Higher = more coherent chunks (topically homogeneous).
    """
    from sklearn.metrics.pairwise import cosine_similarity

    coherences = []
    for start, end in chunk_boundaries:
        if end - start < 2:
            coherences.append(1.0)
            continue
        chunk_emb = embeddings[start:end]
        sim_matrix = cosine_similarity(chunk_emb)
        # Mean of upper triangle (exclude diagonal)
        n = sim_matrix.shape[0]
        upper = sim_matrix[np.triu_indices(n, k=1)]
        coherences.append(float(np.mean(upper)))

    return float(np.mean(coherences)) if coherences else 0.0


def activation_density_variance(
    activation: NDArray[np.float64],
    token_costs: NDArray[np.int64],
) -> float:
    """Variance of activation density ρ(v) = a*(v) / τ(v).

    Higher variance = more opportunity for density-based retrieval
    to outperform naive top-k. When variance is zero (fixed chunk sizes
    and uniform activation), greedy density ≡ top-k.
    """
    active = activation > 0
    if not np.any(active):
        return 0.0
    densities = activation[active] / token_costs[active].astype(float)
    return float(np.var(densities))


def hop_recall_curve(
    retrieved: list[int],
    relevant_by_hop: dict[int, set[int]],
) -> dict[int, float]:
    """Recall broken down by hop distance from query.

    relevant_by_hop: {hop_distance: set of relevant node indices at that distance}

    Returns: {hop_distance: recall} for each hop distance.
    """
    curve = {}
    for hop, nodes in relevant_by_hop.items():
        if not nodes:
            continue
        hits = len(set(retrieved) & nodes)
        curve[hop] = hits / len(nodes)
    return curve
