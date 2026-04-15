"""
Retrieval functions: graph-based (Co-Wiki) vs vector-based (RAG baseline).

Graph retrieval:  R*(q, G, B) = argmax_{S⊆V, Στ(v)≤B} Σ a*(v)
                  Solved via greedy activation density ρ(v) = a*(v) / τ(v)

Vector retrieval: R_vec(q, D, B) = top-⌊B/L⌋ chunks by cosine similarity
"""

from __future__ import annotations

import numpy as np
from numpy.typing import NDArray
from sklearn.feature_extraction.text import TfidfVectorizer
from sklearn.metrics.pairwise import cosine_similarity

from .graph import CoWikiGraph
from .activation import spreading_activation


# ---------------------------------------------------------------------------
# Graph-based retrieval (Co-Wiki)
# ---------------------------------------------------------------------------

def _greedy_by_density(
    activation: NDArray[np.float64],
    token_costs: NDArray[np.int64],
    budget: int,
) -> tuple[list[int], float]:
    """Pure greedy-by-density. Internal helper."""
    n = len(activation)
    densities = np.zeros(n)
    nonzero = token_costs > 0
    densities[nonzero] = activation[nonzero] / token_costs[nonzero]

    order = np.argsort(-densities)

    selected = []
    total_tokens = 0
    total_activation = 0.0

    for idx in order:
        idx = int(idx)
        if activation[idx] <= 0:
            continue
        cost = int(token_costs[idx])
        if total_tokens + cost <= budget:
            selected.append(idx)
            total_tokens += cost
            total_activation += float(activation[idx])

    return selected, total_activation


def greedy_retrieval(
    activation: NDArray[np.float64],
    token_costs: NDArray[np.int64],
    budget: int,
) -> tuple[list[int], float]:
    """Modified greedy knapsack retrieval — guarantees ≥ ½ OPT.

    Standard knapsack result: max(greedy_by_density, best_single_item) ≥ ½ OPT.

    The pure density-greedy can fail when one large, high-value item
    dominates but gets skipped in favor of many small items that
    collectively have less total value. Taking the max with the
    best single item that fits fixes this.

    Args:
        activation: Converged activation vector a*.
        token_costs: Token cost per article.
        budget: Maximum total tokens B.

    Returns:
        (selected_indices, total_activation)
    """
    # Strategy 1: greedy by density
    density_sel, density_val = _greedy_by_density(activation, token_costs, budget)

    # Strategy 2: best single item that fits
    best_single_idx = -1
    best_single_val = 0.0
    for i in range(len(activation)):
        if token_costs[i] <= budget and activation[i] > best_single_val:
            best_single_val = float(activation[i])
            best_single_idx = i

    # Return whichever is better
    if best_single_val > density_val and best_single_idx >= 0:
        return [best_single_idx], best_single_val
    return density_sel, density_val


def optimal_retrieval_bruteforce(
    activation: NDArray[np.float64],
    token_costs: NDArray[np.int64],
    budget: int,
) -> tuple[list[int], float]:
    """Exact optimal retrieval via brute-force enumeration.

    Only feasible for small n (≤ 20). Used to verify greedy bound.
    """
    n = len(activation)
    assert n <= 20, f"Brute-force only feasible for n ≤ 20, got {n}"

    best_value = 0.0
    best_set: list[int] = []

    for mask in range(1 << n):
        indices = [i for i in range(n) if mask & (1 << i)]
        total_cost = sum(int(token_costs[i]) for i in indices)
        if total_cost > budget:
            continue
        total_value = sum(float(activation[i]) for i in indices)
        if total_value > best_value:
            best_value = total_value
            best_set = indices

    return best_set, best_value


def graph_retrieve(
    graph: CoWikiGraph,
    a_initial: NDArray[np.float64],
    budget: int,
    d: float = 0.8,
    theta: float = 0.01,
) -> tuple[list[int], NDArray[np.float64]]:
    """Full Co-Wiki retrieval pipeline: activate → spread → retrieve.

    Returns:
        (selected_indices, activation_vector)
    """
    a_star, _, _ = spreading_activation(graph, a_initial, d=d, theta=theta)
    selected, _ = greedy_retrieval(a_star, graph.token_costs, budget)
    return selected, a_star


# ---------------------------------------------------------------------------
# Vector-based retrieval (RAG baseline)
# ---------------------------------------------------------------------------

def vector_retrieve(
    query_text: str,
    chunk_texts: list[str],
    chunk_size: int,
    budget: int,
) -> list[int]:
    """Standard RAG retrieval: TF-IDF + cosine similarity, top-k.

    All chunks are assumed to be fixed-size (chunk_size tokens).
    Select top-⌊B/L⌋ chunks by cosine similarity.

    Returns:
        List of selected chunk indices.
    """
    k = budget // chunk_size
    if k == 0:
        return []

    vectorizer = TfidfVectorizer()
    tfidf = vectorizer.fit_transform([query_text] + chunk_texts)
    query_vec = tfidf[0:1]
    chunk_vecs = tfidf[1:]
    similarities = cosine_similarity(query_vec, chunk_vecs).flatten()

    top_k = np.argsort(-similarities)[:k]
    return list(top_k)


def vector_retrieve_from_embeddings(
    query_embedding: NDArray[np.float64],
    chunk_embeddings: NDArray[np.float64],
    chunk_size: int,
    budget: int,
) -> list[int]:
    """Vector retrieval from pre-computed embeddings.

    Useful for hypothesis tests where we control embeddings directly.
    """
    k = budget // chunk_size
    if k == 0:
        return []

    similarities = cosine_similarity(
        query_embedding.reshape(1, -1),
        chunk_embeddings,
    ).flatten()

    top_k = np.argsort(-similarities)[:k]
    return list(top_k)
