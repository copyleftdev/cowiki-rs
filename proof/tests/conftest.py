"""
Shared Hypothesis strategies for generating Co-Wiki test data.

These strategies generate random graphs, activation vectors, token costs,
and structured scenarios (e.g., planted multi-hop relevance chains).
"""

from __future__ import annotations

import numpy as np
import hypothesis.strategies as st
from hypothesis.extra.numpy import arrays

from cowiki.graph import CoWikiGraph

# ---------------------------------------------------------------------------
# Primitive strategies
# ---------------------------------------------------------------------------

# Graph sizes: keep small enough for brute-force verification
graph_sizes = st.integers(min_value=3, max_value=15)

# Propagation factor d ∈ (0.1, 0.95) — avoid extremes
propagation_factors = st.floats(min_value=0.1, max_value=0.95)

# Threshold θ ∈ [0, 0.2] — typical operating range
thresholds = st.floats(min_value=0.0, max_value=0.2)

# Token budgets
budgets = st.integers(min_value=50, max_value=2000)

# Decay rates λ
decay_rates = st.floats(min_value=0.01, max_value=0.5)


# ---------------------------------------------------------------------------
# Composite strategies
# ---------------------------------------------------------------------------

@st.composite
def random_graphs(draw, min_nodes=3, max_nodes=15, edge_prob=0.3):
    """Generate a random CoWikiGraph with Erdős–Rényi edges."""
    n = draw(st.integers(min_value=min_nodes, max_value=max_nodes))

    # Random adjacency: each edge exists with probability edge_prob
    edge_mask = draw(arrays(
        dtype=np.float64,
        shape=(n, n),
        elements=st.floats(min_value=0.0, max_value=1.0),
    ))
    weights = draw(arrays(
        dtype=np.float64,
        shape=(n, n),
        elements=st.floats(min_value=0.1, max_value=2.0),
    ))

    adjacency = np.where(edge_mask < edge_prob, weights, 0.0)
    np.fill_diagonal(adjacency, 0.0)  # No self-loops

    # Token costs: variable sizes (the human-cognitive chunking property)
    token_costs = draw(arrays(
        dtype=np.int64,
        shape=(n,),
        elements=st.integers(min_value=50, max_value=500),
    ))

    return CoWikiGraph(adjacency=adjacency, token_costs=token_costs)


@st.composite
def random_activations(draw, n):
    """Generate a sparse initial activation vector."""
    a = np.zeros(n)
    # Activate 1-3 seed nodes
    n_seeds = draw(st.integers(min_value=1, max_value=min(3, n)))
    seeds = draw(st.lists(
        st.integers(min_value=0, max_value=n - 1),
        min_size=n_seeds, max_size=n_seeds, unique=True,
    ))
    for s in seeds:
        a[s] = draw(st.floats(min_value=0.3, max_value=1.0))
    return a


@st.composite
def activation_pairs(draw, n):
    """Generate two distinct activation vectors for contraction tests."""
    a = draw(arrays(
        dtype=np.float64,
        shape=(n,),
        elements=st.floats(min_value=0.0, max_value=1.0),
    ))
    b = draw(arrays(
        dtype=np.float64,
        shape=(n,),
        elements=st.floats(min_value=0.0, max_value=1.0),
    ))
    return a, b


@st.composite
def chain_graphs(draw, min_length=4, max_length=10):
    """Generate a graph with a planted relevance chain.

    Structure: v₀ → v₁ → v₂ → ... → v_k
    Plus some random noise edges.

    v₀ is the query-adjacent node (hop 0).
    v_k is the multi-hop relevant target.
    All nodes in the chain are "relevant."
    """
    k = draw(st.integers(min_value=min_length, max_value=max_length))
    # Add extra noise nodes
    n_noise = draw(st.integers(min_value=0, max_value=5))
    n = k + n_noise

    adjacency = np.zeros((n, n))

    # Plant the chain
    for i in range(k - 1):
        weight = draw(st.floats(min_value=0.3, max_value=1.5))
        adjacency[i, i + 1] = weight

    # Add random noise edges (not crossing the chain)
    for _ in range(n_noise * 2):
        src = draw(st.integers(min_value=0, max_value=n - 1))
        dst = draw(st.integers(min_value=0, max_value=n - 1))
        if src != dst:
            adjacency[src, dst] = draw(st.floats(min_value=0.1, max_value=0.5))

    token_costs = draw(arrays(
        dtype=np.int64,
        shape=(n,),
        elements=st.integers(min_value=50, max_value=300),
    ))

    graph = CoWikiGraph(adjacency=adjacency, token_costs=token_costs)
    chain_nodes = set(range(k))

    return graph, chain_nodes, k


@st.composite
def clustered_graphs(draw, n_clusters=3, cluster_size=4):
    """Generate a graph with dense intra-cluster and sparse inter-cluster edges.

    Guarantees a bidirectional ring within each cluster so activation can
    always spread internally. Additional random intra-cluster edges added on top.
    """
    n = n_clusters * cluster_size
    adjacency = np.zeros((n, n))

    # Guaranteed intra-cluster ring (bidirectional) so clusters are connected
    for c in range(n_clusters):
        start = c * cluster_size
        for offset in range(cluster_size):
            i = start + offset
            j = start + (offset + 1) % cluster_size
            w = draw(st.floats(min_value=0.5, max_value=2.0))
            adjacency[i, j] = w
            adjacency[j, i] = w

    # Additional random intra-cluster edges (~50%)
    for c in range(n_clusters):
        start = c * cluster_size
        end = start + cluster_size
        for i in range(start, end):
            for j in range(start, end):
                if i != j and adjacency[i, j] == 0:
                    if draw(st.booleans()):
                        adjacency[i, j] = draw(st.floats(min_value=0.5, max_value=2.0))

    # Sparse inter-cluster edges (~10%)
    for i in range(n):
        for j in range(n):
            if i // cluster_size != j // cluster_size and i != j:
                if draw(st.floats(min_value=0.0, max_value=1.0)) < 0.1:
                    adjacency[i, j] = draw(st.floats(min_value=0.1, max_value=0.5))

    token_costs = draw(arrays(
        dtype=np.int64,
        shape=(n,),
        elements=st.integers(min_value=50, max_value=400),
    ))

    graph = CoWikiGraph(adjacency=adjacency, token_costs=token_costs)
    clusters = [set(range(c * cluster_size, (c + 1) * cluster_size))
                for c in range(n_clusters)]

    return graph, clusters
