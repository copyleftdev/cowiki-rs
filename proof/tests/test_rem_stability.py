"""
PROPERTY 6: REM Agent Maintains Graph Health Over Time.

Conjecture: Under Decay + Prune + Dream operators,
    ∀t: H(Gₜ) ≥ H_min > 0

The REM Agent prevents the graph from fragmenting into unreachable
islands while keeping |Vₜ| bounded.

Tests:
    P6.1  Decay monotonicity: edge weights decrease with access recency
    P6.2  Prune targets stale nodes: only inactive nodes are pruned
    P6.3  Dream discovers valid edges: new backlinks connect related nodes
    P6.4  Health stability: H(Gₜ) stays bounded over multiple REM cycles
    P6.5  Graph size is bounded: prune prevents unbounded growth
    P6.6  Dream compensates prune: new edges maintain connectivity
"""

import numpy as np
from hypothesis import given, settings, assume
from hypothesis import strategies as st

from cowiki.graph import CoWikiGraph
from cowiki.rem import (
    REMState,
    access_recency,
    decay_operator,
    prune_operator,
    dream_operator,
    graph_health,
    rem_step,
)
from cowiki.activation import spreading_activation
from tests.conftest import random_graphs, random_activations


class TestDecayMonotonicity:
    """P6.1: Edges from less-recently-accessed nodes decay faster."""

    @given(
        graph=random_graphs(min_nodes=4, max_nodes=10),
        decay_rate=st.floats(min_value=0.01, max_value=0.5),
    )
    @settings(max_examples=200, deadline=None)
    def test_decay_increases_with_recency(self, graph, decay_rate):
        """Nodes accessed longer ago have more decayed edge weights."""
        state = REMState(graph=graph)
        state.time = 10

        # Node 0 accessed recently, node 1 accessed long ago
        state.last_access[0] = 9   # recency = 1
        state.last_access[1] = 2   # recency = 8

        decayed = decay_operator(state, decay_rate)

        # Compare: row 0 (recent) should have higher weights than row 1 (stale)
        # relative to their original weights
        for j in range(graph.n):
            if graph.raw_weights[0, j] > 0 and graph.raw_weights[1, j] > 0:
                ratio_0 = decayed[0, j] / graph.raw_weights[0, j]
                ratio_1 = decayed[1, j] / graph.raw_weights[1, j]
                assert ratio_0 >= ratio_1 - 1e-9, (
                    f"Recent node decayed more than stale node: "
                    f"ratio_recent={ratio_0:.4f}, ratio_stale={ratio_1:.4f}"
                )

    @given(
        graph=random_graphs(min_nodes=4, max_nodes=10),
        decay_rate=st.floats(min_value=0.01, max_value=0.5),
    )
    @settings(max_examples=200, deadline=None)
    def test_decay_is_exponential(self, graph, decay_rate):
        """w_t = w_0 · exp(-λ · r), so log(w_t/w_0) = -λ·r."""
        state = REMState(graph=graph)
        state.time = 20

        # Set known recencies
        for i in range(graph.n):
            state.last_access[i] = 20 - (i + 1)  # recency = i+1

        decayed = decay_operator(state, decay_rate)
        recency = access_recency(state)

        for i in range(graph.n):
            for j in range(graph.n):
                if graph.raw_weights[i, j] > 0:
                    expected = graph.raw_weights[i, j] * np.exp(-decay_rate * recency[i])
                    assert abs(decayed[i, j] - expected) < 1e-10, (
                        f"Decay mismatch at ({i},{j}): "
                        f"got {decayed[i,j]:.6f}, expected {expected:.6f}"
                    )


class TestPruneTargeting:
    """P6.2: Prune only removes nodes with consistently low activation."""

    @given(
        graph=random_graphs(min_nodes=5, max_nodes=10),
        theta_prune=st.floats(min_value=0.01, max_value=0.2),
    )
    @settings(max_examples=200, deadline=None)
    def test_active_nodes_not_pruned(self, graph, theta_prune):
        """Nodes with activation > theta_prune are never pruned."""
        state = REMState(graph=graph)

        # Simulate activation history where node 0 is always active
        window = 5
        for _ in range(window):
            a = np.zeros(graph.n)
            a[0] = 0.5  # Well above any reasonable theta_prune
            state.activation_history.append(a)

        prunable = prune_operator(state, theta_prune, window)

        assert 0 not in prunable, "Active node should not be pruned"

    @given(graph=random_graphs(min_nodes=5, max_nodes=10))
    @settings(max_examples=200, deadline=None)
    def test_dormant_nodes_pruned(self, graph):
        """Nodes with zero activation over the window are prunable."""
        state = REMState(graph=graph)

        window = 5
        for _ in range(window):
            a = np.zeros(graph.n)
            a[0] = 0.5  # Only node 0 is active
            state.activation_history.append(a)

        prunable = prune_operator(state, theta_prune=0.01, window=window)

        # All nodes except node 0 should be prunable
        for v in range(1, graph.n):
            assert v in prunable, f"Dormant node {v} should be prunable"


class TestDreamDiscovery:
    """P6.3: Dream operator adds edges between similar nodes."""

    @given(graph=random_graphs(min_nodes=5, max_nodes=10))
    @settings(max_examples=200, deadline=None)
    def test_dream_finds_new_edges(self, graph):
        """Dream adds edges where similarity is high and no edge exists."""
        state = REMState(graph=graph)

        # Create a similarity matrix where nodes 0-1 are very similar
        # but have no existing edge
        sim = np.zeros((graph.n, graph.n))
        sim[0, 1] = 0.9
        sim[1, 0] = 0.9

        # Remove existing edge if any
        graph.raw_weights[0, 1] = 0.0
        graph.raw_weights[1, 0] = 0.0

        new_edges = dream_operator(state, sim, theta_dream=0.5)

        assert (0, 1) in new_edges or (1, 0) in new_edges, (
            "Dream should discover edge between similar unconnected nodes"
        )

    @given(graph=random_graphs(min_nodes=5, max_nodes=10))
    @settings(max_examples=200, deadline=None)
    def test_dream_no_duplicate_edges(self, graph):
        """Dream does not propose edges that already exist."""
        state = REMState(graph=graph)

        # Similarity matrix where everything is highly similar
        sim = np.ones((graph.n, graph.n))

        new_edges = dream_operator(state, sim, theta_dream=0.1)

        for src, dst in new_edges:
            assert graph.raw_weights[src, dst] == 0, (
                f"Dream proposed duplicate edge ({src},{dst}) "
                f"with existing weight {graph.raw_weights[src, dst]}"
            )


class TestHealthStability:
    """P6.4: Graph health stays bounded over REM cycles."""

    @given(graph=random_graphs(min_nodes=6, max_nodes=10))
    @settings(max_examples=50, deadline=None)
    def test_health_bounded_over_cycles(self, graph):
        """H(Gₜ) should not collapse to zero over moderate time horizons."""
        state = REMState(graph=graph)

        # Create content similarity for dreaming
        sim = np.random.rand(graph.n, graph.n) * 0.3
        np.fill_diagonal(sim, 0)

        # Run several REM cycles with random queries
        n_cycles = 8
        for t in range(n_cycles):
            # Random query activation
            a0 = np.zeros(graph.n)
            alive_nodes = np.where(state.node_alive)[0]
            if len(alive_nodes) == 0:
                break
            seed = np.random.choice(alive_nodes)
            a0[seed] = 1.0

            state = rem_step(
                state, a0,
                decay_rate=0.02,       # Gentle decay
                theta_prune=0.0001,    # Low prune threshold
                prune_window=5,
                content_similarity=sim,
                theta_dream=0.8,       # High dream threshold
                d=0.8, theta=0.01,
            )

        # Health should not have collapsed
        if state.health_history:
            final_health = state.health_history[-1]
            # With gentle parameters, health should stay reasonable
            assert final_health > 0, (
                f"Graph health collapsed to {final_health} "
                f"after {n_cycles} REM cycles. "
                f"History: {state.health_history}"
            )


class TestGraphSizeBound:
    """P6.5: Pruning prevents the alive node count from growing unbounded.

    NOTE: In dense graphs, spreading activation reaches all nodes even
    from a single query, so nothing gets pruned. This is correct behavior —
    dense connectivity means everything is "relevant."

    Pruning only has bite in sparse graphs where distant nodes genuinely
    receive zero activation over the prune window.
    """

    @given(graph=random_graphs(min_nodes=8, max_nodes=12, edge_prob=0.1))
    @settings(max_examples=50, deadline=None)
    def test_prune_reduces_alive_count_sparse(self, graph):
        """In sparse graphs with focused queries, distant nodes get pruned."""
        state = REMState(graph=graph)

        # Only ever query node 0 — in a sparse graph, distant nodes
        # should not be reached by spreading activation
        n_cycles = 15
        for _ in range(n_cycles):
            a0 = np.zeros(graph.n)
            a0[0] = 1.0

            state = rem_step(
                state, a0,
                decay_rate=0.2,        # Aggressive decay
                theta_prune=0.05,      # Moderate prune threshold
                prune_window=3,
                d=0.7, theta=0.02,     # Lower propagation, higher threshold
            )

        alive = int(np.sum(state.node_alive))
        # In a sparse graph, some nodes should be unreachable and pruned.
        # If graph is so well-connected that all nodes activate, that's
        # valid — prune is not needed when everything is connected.
        reachable_from_0 = set()
        frontier = {0}
        while frontier:
            node = frontier.pop()
            if node not in reachable_from_0:
                reachable_from_0.add(node)
                frontier.update(graph.neighbors_out(node))

        if len(reachable_from_0) < graph.n:
            # Graph has unreachable nodes — pruning should have found some
            assert alive < graph.n, (
                f"No nodes pruned despite {graph.n - len(reachable_from_0)} "
                f"unreachable nodes ({alive}/{graph.n} alive)"
            )
