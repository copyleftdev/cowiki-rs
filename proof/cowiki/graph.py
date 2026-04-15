"""
Co-Wiki Knowledge Graph.

G = (V, E, w, τ, κ)

V = articles (nodes)
E = directed edges (backlinks, category co-membership)
w: E → ℝ⁺ (edge weight / association strength)
τ: V → ℕ (token cost per article)
κ: V → 2^C (category assignment)
"""

from __future__ import annotations

import numpy as np
from numpy.typing import NDArray


class CoWikiGraph:
    """Weighted directed graph representing a Co-Wiki knowledge base."""

    def __init__(
        self,
        adjacency: NDArray[np.float64],
        token_costs: NDArray[np.int64],
        categories: list[set[int]] | None = None,
    ):
        """
        Args:
            adjacency: Raw weight matrix W_raw[i,j] = weight of edge i → j.
                       Zero means no edge. Will be column-normalized internally.
            token_costs: τ(v) for each node — number of tokens in the article.
            categories: κ(v) — set of category indices per node. Optional.
        """
        n = adjacency.shape[0]
        assert adjacency.shape == (n, n), "Adjacency must be square"
        assert token_costs.shape == (n,), "Token costs must match node count"
        assert np.all(adjacency >= 0), "Weights must be non-negative"
        assert np.all(token_costs > 0), "Token costs must be positive"

        self.n = n
        self.raw_weights = adjacency.copy()
        self.token_costs = token_costs.copy()
        self.categories = categories or [set() for _ in range(n)]

        # Column-stochastic normalization: W[i,j] = w(i,j) / Σ_k w(i,k)
        # This normalizes by out-degree of source node i.
        self.W = self._column_stochastize(adjacency)

    @staticmethod
    def _column_stochastize(W_raw: NDArray[np.float64]) -> NDArray[np.float64]:
        """Normalize so each row sums to 1 (out-degree normalization).

        Rows with zero out-degree remain zero (isolated source nodes).
        Returns W where W[i,j] = w_raw[i,j] / Σ_k w_raw[i,k].
        """
        W = W_raw.copy()
        row_sums = W.sum(axis=1, keepdims=True)
        # Avoid division by zero for nodes with no outgoing edges
        row_sums = np.where(row_sums == 0, 1.0, row_sums)
        W /= row_sums
        return W

    @property
    def edges(self) -> list[tuple[int, int, float]]:
        """Return list of (src, dst, weight) for all edges."""
        rows, cols = np.nonzero(self.raw_weights)
        return [(int(r), int(c), float(self.raw_weights[r, c]))
                for r, c in zip(rows, cols)]

    def neighbors_out(self, v: int) -> list[int]:
        """Outgoing neighbors of node v."""
        return list(np.nonzero(self.raw_weights[v])[0])

    def neighbors_in(self, v: int) -> list[int]:
        """Incoming neighbors of node v (nodes that link TO v)."""
        return list(np.nonzero(self.raw_weights[:, v])[0])

    def shortest_path_length(self, src: int, dst: int) -> int | None:
        """BFS shortest path length. Returns None if unreachable."""
        if src == dst:
            return 0
        visited = {src}
        frontier = [src]
        depth = 0
        while frontier:
            depth += 1
            next_frontier = []
            for node in frontier:
                for nb in self.neighbors_out(node):
                    if nb == dst:
                        return depth
                    if nb not in visited:
                        visited.add(nb)
                        next_frontier.append(nb)
            frontier = next_frontier
        return None
