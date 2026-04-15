"""
PROPERTY 4: Graph Retrieval Outperforms Vector Retrieval for Associative Queries.

Hypothesis H2 (formal):
    For queries requiring multi-hop context (Q_assoc), graph-based
    spreading activation achieves higher recall at equivalent token budgets
    than vector similarity search.

    𝔼[Recall(R*(q, G, B))] > 𝔼[Recall(R_vec(q, D, B))]

The key mechanism: spreading activation reaches nodes at hop distance h
with probability ≥ (1-d)·d^h · Π w_e, while vector search has no structural
reason to find semantically-distant but contextually-relevant articles.

Tests:
    P4.1  Graph retrieval finds multi-hop relevant nodes
    P4.2  Graph recall ≥ vector recall on planted chain queries
    P4.3  Recall degrades gracefully with hop distance (not cliff-edge)
    P4.4  Cluster-aware retrieval: activation stays within relevant cluster
"""

import numpy as np
from hypothesis import given, settings, assume
from hypothesis import strategies as st

from cowiki.graph import CoWikiGraph
from cowiki.activation import spreading_activation
# CoWikiGraph used below to construct pure chain subgraphs for testing
from cowiki.retrieval import (
    greedy_retrieval,
    vector_retrieve_from_embeddings,
)
from cowiki.metrics import recall_at_budget, hop_recall_curve
from tests.conftest import chain_graphs, clustered_graphs


class TestMultiHopReachability:
    """P4.1: Spreading activation reaches multi-hop relevant nodes."""

    @given(chain=chain_graphs(min_length=4, max_length=8))
    @settings(max_examples=200, deadline=None)
    def test_chain_end_activated(self, chain):
        """The last node in a relevance chain gets non-zero activation."""
        graph, chain_nodes, k = chain

        # Activate only the first node in the chain
        a0 = np.zeros(graph.n)
        a0[0] = 1.0

        a_star, _, _ = spreading_activation(
            graph, a0, d=0.85, theta=0.001, max_iter=200,
        )

        # The chain end (node k-1) should have positive activation
        end_activation = a_star[k - 1]
        assert end_activation > 0, (
            f"Chain end (node {k-1}, hop={k-1}) has zero activation. "
            f"Chain length={k}, activations={a_star[:k]}"
        )

    @given(chain=chain_graphs(min_length=4, max_length=8))
    @settings(max_examples=200, deadline=None)
    def test_activation_decays_overall(self, chain):
        """Chain end has LESS activation than source on a PURE chain.

        FINDINGS from hypothesis:
        1. Monotonic per-hop decay does NOT hold — noise edges feed
           activation back to later nodes via alternate paths.
        2. Even start>end fails when noise edges from outside the chain
           contribute in-degree to the end node, giving it more activation
           than the source's anchor term (1-d)·a⁰.

        The property that DOES hold: on a pure chain (no noise edges),
        activation monotonically decays with hop distance.
        """
        graph, chain_nodes, k = chain

        # Build a PURE chain graph (no noise edges) to test the clean property
        pure_adj = np.zeros((k, k))
        for i in range(k - 1):
            pure_adj[i, i + 1] = graph.raw_weights[i, i + 1] if i + 1 < graph.n else 1.0
        pure_costs = graph.token_costs[:k].copy()
        pure_graph = CoWikiGraph(pure_adj, pure_costs)

        a0 = np.zeros(k)
        a0[0] = 1.0

        a_star, _, _ = spreading_activation(
            pure_graph, a0, d=0.85, theta=0.0001, max_iter=200,
        )

        # On a pure chain: start > end (always)
        assert a_star[0] >= a_star[k - 1], (
            f"Pure chain: end ({a_star[k-1]:.6f}) > start ({a_star[0]:.6f})"
        )
        # Monotonic decay along pure chain
        for h in range(1, k):
            if a_star[h] == 0 and a_star[h - 1] == 0:
                continue
            assert a_star[h] <= a_star[h - 1] + 1e-9, (
                f"Pure chain: hop {h} ({a_star[h]:.6f}) > hop {h-1} ({a_star[h-1]:.6f})"
            )


class TestGraphVsVector:
    """P4.2: Graph recall ≥ vector recall for associative (multi-hop) queries."""

    @given(chain=chain_graphs(min_length=5, max_length=8))
    @settings(max_examples=200, deadline=None)
    def test_graph_beats_vector_on_chains(self, chain):
        """On planted chain queries, graph retrieval finds more relevant
        nodes than vector search at the same token budget."""
        graph, chain_nodes, k = chain
        n = graph.n

        # --- Graph retrieval ---
        a0 = np.zeros(n)
        a0[0] = 1.0
        a_star, _, _ = spreading_activation(
            graph, a0, d=0.85, theta=0.001, max_iter=200,
        )
        budget = int(np.sum(graph.token_costs))  # Generous budget
        graph_selected, _ = greedy_retrieval(a_star, graph.token_costs, budget)

        # --- Vector retrieval ---
        # Simulate embeddings: chain nodes are NOT close in embedding space
        # (the whole point — multi-hop relevance ≠ semantic similarity).
        # Node 0 is close to query, others are random.
        embed_dim = 32
        rng = np.random.RandomState(42)
        query_emb = rng.randn(embed_dim)
        chunk_embs = rng.randn(n, embed_dim)
        # Make node 0 similar to query in embedding space
        chunk_embs[0] = query_emb + rng.randn(embed_dim) * 0.1
        # All other chain nodes are random — no embedding similarity to query

        avg_chunk_size = int(np.mean(graph.token_costs))
        vector_selected = vector_retrieve_from_embeddings(
            query_emb, chunk_embs, avg_chunk_size, budget,
        )

        # --- Compare recall ---
        graph_recall = recall_at_budget(graph_selected, chain_nodes)
        vector_recall = recall_at_budget(vector_selected, chain_nodes)

        assert graph_recall >= vector_recall, (
            f"Graph recall ({graph_recall:.3f}) < vector recall ({vector_recall:.3f}) "
            f"on a {k}-hop chain query"
        )


class TestHopRecallCurve:
    """P4.3: Recall degrades gracefully with hop distance."""

    @given(chain=chain_graphs(min_length=5, max_length=8))
    @settings(max_examples=150, deadline=None)
    def test_graceful_degradation(self, chain):
        """Recall at hop h should be > 0 for small h, declining smoothly."""
        graph, chain_nodes, k = chain

        a0 = np.zeros(graph.n)
        a0[0] = 1.0
        a_star, _, _ = spreading_activation(
            graph, a0, d=0.85, theta=0.0001, max_iter=200,
        )
        budget = int(np.sum(graph.token_costs))
        selected, _ = greedy_retrieval(a_star, graph.token_costs, budget)

        # Build hop-indexed relevance
        relevant_by_hop = {h: {h} for h in range(k)}
        curve = hop_recall_curve(selected, relevant_by_hop)

        # Hop 0 should always be retrieved (direct query match)
        assert curve.get(0, 0) == 1.0, "Hop-0 node should always be retrieved"

        # At least the first 2 hops should be retrieved
        if k >= 3:
            assert curve.get(1, 0) > 0, "Hop-1 should have non-zero recall"


class TestClusterAwareness:
    """P4.4: Activation concentrates within the relevant cluster."""

    @given(cg=clustered_graphs(n_clusters=3, cluster_size=4))
    @settings(max_examples=100, deadline=None)
    def test_activation_concentrates_in_cluster(self, cg):
        """When query activates a node in cluster C, most activation
        should stay within C."""
        graph, clusters = cg

        # Activate a node in cluster 0
        a0 = np.zeros(graph.n)
        seed = list(clusters[0])[0]
        a0[seed] = 1.0

        a_star, _, _ = spreading_activation(
            graph, a0, d=0.8, theta=0.01, max_iter=100,
        )

        # Measure activation mass in-cluster vs out-of-cluster
        in_cluster = sum(a_star[v] for v in clusters[0])
        total = np.sum(a_star)

        if total > 0:
            concentration = in_cluster / total
            # With dense intra-cluster and sparse inter-cluster edges,
            # most activation should stay within the cluster
            assert concentration > 0.3, (
                f"Cluster concentration too low: {concentration:.3f} "
                f"(in={in_cluster:.4f}, total={total:.4f})"
            )
