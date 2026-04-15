"""
REM Agent: Pruning, Decay, and Dreaming operators on the Co-Wiki graph.

Time evolution Gₜ → Gₜ₊₁ via three operators:

Decay:   w_t(e) = w₀(e) · exp(-λ · r(v_src, t))
         where r(v, t) = t - t_last(v)

Prune:   remove v if max activation over window < θ_prune

Dream:   add edge (u,v) if content_sim(u,v) > θ_dream and (u,v) ∉ E

Health metric:
    H(Gₜ) = |{v ∈ Vₜ : ∃q, a*(v) > θ}| / |Vₜ|
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from numpy.typing import NDArray

from .graph import CoWikiGraph
from .activation import spreading_activation


@dataclass
class REMState:
    """Mutable state tracked by the REM Agent across time steps."""
    graph: CoWikiGraph
    time: int = 0
    last_access: NDArray[np.int64] = field(init=False)
    activation_history: list[NDArray[np.float64]] = field(default_factory=list)
    health_history: list[float] = field(default_factory=list)
    node_alive: NDArray[np.bool_] = field(init=False)

    def __post_init__(self):
        self.last_access = np.zeros(self.graph.n, dtype=np.int64)
        self.node_alive = np.ones(self.graph.n, dtype=bool)


def access_recency(state: REMState) -> NDArray[np.float64]:
    """r(v, t) = t - t_last(v) for all nodes."""
    return (state.time - state.last_access).astype(np.float64)


def decay_operator(
    state: REMState,
    decay_rate: float,
) -> NDArray[np.float64]:
    """Apply exponential decay to edge weights based on source node recency.

    w_t(i,j) = w_raw(i,j) · exp(-λ · r(v_i, t))

    Returns the decayed weight matrix (not normalized).
    """
    r = access_recency(state)
    decay_factors = np.exp(-decay_rate * r)
    # Each row i is scaled by the decay factor of source node i
    decayed = state.graph.raw_weights * decay_factors[:, np.newaxis]
    return decayed


def prune_operator(
    state: REMState,
    theta_prune: float,
    window: int,
) -> list[int]:
    """Identify nodes to prune: those whose max activation over the
    last `window` periods never exceeded theta_prune.

    Returns list of node indices to prune.
    """
    if len(state.activation_history) < window:
        return []

    recent = state.activation_history[-window:]
    prunable = []

    for v in range(state.graph.n):
        if not state.node_alive[v]:
            continue
        max_activation = max(float(a[v]) for a in recent)
        if max_activation < theta_prune:
            prunable.append(v)

    return prunable


def dream_operator(
    state: REMState,
    content_similarity: NDArray[np.float64],
    theta_dream: float,
) -> list[tuple[int, int]]:
    """Discover new backlinks: edges where content similarity exceeds
    theta_dream but no edge currently exists.

    Args:
        content_similarity: Precomputed n×n similarity matrix.
        theta_dream: Threshold for creating a new edge.

    Returns:
        List of (src, dst) edges to add.
    """
    new_edges = []
    n = state.graph.n

    for i in range(n):
        if not state.node_alive[i]:
            continue
        for j in range(n):
            if i == j or not state.node_alive[j]:
                continue
            if state.graph.raw_weights[i, j] > 0:
                continue  # Edge already exists
            if content_similarity[i, j] > theta_dream:
                new_edges.append((i, j))

    return new_edges


def graph_health(
    graph: CoWikiGraph,
    node_alive: NDArray[np.bool_],
    n_probe_queries: int = 10,
    activation_threshold: float = 0.01,
    d: float = 0.8,
    theta: float = 0.01,
) -> float:
    """Compute retrievable coverage H(Gₜ).

    H(Gₜ) = |{v ∈ alive : ∃ probe query where a*(v) > threshold}| / |alive|

    Uses random probe queries to estimate reachability.
    """
    alive_count = int(np.sum(node_alive))
    if alive_count == 0:
        return 0.0

    ever_activated = np.zeros(graph.n, dtype=bool)

    for _ in range(n_probe_queries):
        # Random sparse initial activation
        a0 = np.zeros(graph.n)
        seed_node = np.random.choice(np.where(node_alive)[0])
        a0[seed_node] = 1.0

        a_star, _, _ = spreading_activation(
            graph, a0, d=d, theta=theta, max_iter=50
        )
        ever_activated |= (a_star > activation_threshold) & node_alive

    reachable = int(np.sum(ever_activated))
    return reachable / alive_count


def rem_step(
    state: REMState,
    query_activation: NDArray[np.float64],
    decay_rate: float = 0.05,
    theta_prune: float = 0.001,
    prune_window: int = 5,
    content_similarity: NDArray[np.float64] | None = None,
    theta_dream: float = 0.5,
    d: float = 0.8,
    theta: float = 0.01,
) -> REMState:
    """Execute one full REM cycle: access → activate → decay → prune → dream.

    Mutates and returns the state.
    """
    state.time += 1

    # 1. Run activation for this time step's query
    a_star, _, _ = spreading_activation(
        state.graph, query_activation, d=d, theta=theta
    )
    state.activation_history.append(a_star)

    # 2. Update access times for activated nodes
    activated_mask = a_star > theta
    state.last_access[activated_mask] = state.time

    # 3. Decay: update edge weights
    decayed_weights = decay_operator(state, decay_rate)
    state.graph = CoWikiGraph(
        adjacency=decayed_weights,
        token_costs=state.graph.token_costs,
        categories=state.graph.categories,
    )

    # 4. Prune
    prunable = prune_operator(state, theta_prune, prune_window)
    for v in prunable:
        state.node_alive[v] = False

    # 5. Dream (if similarity matrix provided)
    if content_similarity is not None:
        new_edges = dream_operator(state, content_similarity, theta_dream)
        if new_edges:
            new_weights = state.graph.raw_weights.copy()
            for src, dst in new_edges:
                new_weights[src, dst] = 0.5  # Default weight for discovered edges
            # Denormalize to get back to raw scale, then re-add
            row_sums = state.graph.raw_weights.sum(axis=1, keepdims=True)
            row_sums = np.where(row_sums == 0, 1.0, row_sums)
            state.graph = CoWikiGraph(
                adjacency=new_weights,
                token_costs=state.graph.token_costs,
                categories=state.graph.categories,
            )

    # 6. Compute and record health
    health = graph_health(
        state.graph, state.node_alive,
        n_probe_queries=5, activation_threshold=theta, d=d, theta=theta,
    )
    state.health_history.append(health)

    return state
