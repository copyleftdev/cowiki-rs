"""
Spreading Activation on the Co-Wiki Graph.

Two operator variants:

Linear (θ=0):
    T(a) = (1 - d) · a⁰  +  d · Wᵀ · a
    Provably contracting: ‖T(a)-T(b)‖₁ ≤ d·‖a-b‖₁

Thresholded (θ>0):
    T(a) = (1 - d) · a⁰  +  d · Wᵀ · f(a)
    where f zeros activations below θ.
    NOT a strict contraction (f is non-expansive only away from θ).
    Still converges empirically; contraction holds for the linear core.

Convergence of the linear operator is guaranteed by Banach fixed-point
theorem when d < 1 and W is column-stochastic.
"""

from __future__ import annotations

import numpy as np
from numpy.typing import NDArray

from .graph import CoWikiGraph


def threshold(a: NDArray[np.float64], theta: float) -> NDArray[np.float64]:
    """Hard threshold: zero out activations below θ.

    NOTE: This function is NOT non-expansive (not Lipschitz-1).
    At the boundary a[j]≈θ, it can amplify small differences.
    Contraction proofs apply to the linear (θ=0) operator only.
    """
    return np.where(a >= theta, a, 0.0)


def activation_step(
    a_current: NDArray[np.float64],
    a_initial: NDArray[np.float64],
    W: NDArray[np.float64],
    d: float,
    theta: float,
) -> NDArray[np.float64]:
    """Single step of the spreading activation operator T.

    T(a) = (1 - d) · a⁰  +  d · Wᵀ · f(a)
    """
    f_a = threshold(a_current, theta)
    # W^T · f(a): activation spreads along incoming edges
    spread = W.T @ f_a
    return (1 - d) * a_initial + d * spread


def linear_activation_step(
    a_current: NDArray[np.float64],
    a_initial: NDArray[np.float64],
    W: NDArray[np.float64],
    d: float,
) -> NDArray[np.float64]:
    """Linear operator (no threshold) — provably contracting.

    T_lin(a) = (1 - d) · a⁰  +  d · Wᵀ · a
    """
    spread = W.T @ a_current
    return (1 - d) * a_initial + d * spread


def spreading_activation(
    graph: CoWikiGraph,
    a_initial: NDArray[np.float64],
    d: float = 0.8,
    theta: float = 0.01,
    max_iter: int = 100,
    epsilon: float = 1e-8,
) -> tuple[NDArray[np.float64], int, list[float]]:
    """Run spreading activation to convergence.

    Args:
        graph: The Co-Wiki knowledge graph.
        a_initial: Initial activation vector a⁰ ∈ [0,1]ⁿ.
        d: Propagation factor. Higher d = activation spreads further.
        theta: Firing threshold. Activations below this are zeroed.
        max_iter: Maximum iterations before forced stop.
        epsilon: Convergence tolerance (L1 norm of difference).

    Returns:
        (a_star, iterations, residuals):
            a_star     — converged activation vector
            iterations — number of iterations taken
            residuals  — L1 residual at each step
    """
    assert 0 < d < 1, f"Propagation factor d must be in (0,1), got {d}"
    assert theta >= 0, f"Threshold must be non-negative, got {theta}"
    assert a_initial.shape == (graph.n,), "Initial activation shape mismatch"

    a = a_initial.copy()
    residuals = []

    for t in range(max_iter):
        a_next = activation_step(a, a_initial, graph.W, d, theta)
        residual = float(np.sum(np.abs(a_next - a)))
        residuals.append(residual)

        if residual < epsilon:
            return a_next, t + 1, residuals

        a = a_next

    return a, max_iter, residuals


def contraction_distance(
    a: NDArray[np.float64],
    b: NDArray[np.float64],
    a_initial: NDArray[np.float64],
    W: NDArray[np.float64],
    d: float,
    theta: float,
) -> tuple[float, float]:
    """Compute ‖T(a) - T(b)‖₁ and d · ‖a - b‖₁ for the THRESHOLDED operator.

    NOTE: This inequality does NOT always hold with hard threshold.
    The threshold function can amplify differences at the boundary.
    Use linear_contraction_distance for the provable bound.
    """
    Ta = activation_step(a, a_initial, W, d, theta)
    Tb = activation_step(b, a_initial, W, d, theta)
    lhs = float(np.sum(np.abs(Ta - Tb)))
    rhs = d * float(np.sum(np.abs(a - b)))
    return lhs, rhs


def linear_contraction_distance(
    a: NDArray[np.float64],
    b: NDArray[np.float64],
    a_initial: NDArray[np.float64],
    W: NDArray[np.float64],
    d: float,
) -> tuple[float, float]:
    """Compute ‖T_lin(a) - T_lin(b)‖₁ and d · ‖a - b‖₁.

    For the linear operator (no threshold), contraction is provable:
    T_lin(a) - T_lin(b) = d · Wᵀ · (a - b)
    ‖T_lin(a) - T_lin(b)‖₁ = d · ‖Wᵀ(a-b)‖₁ ≤ d · ‖a-b‖₁

    since W is column-stochastic → Wᵀ has ‖·‖₁ operator norm ≤ 1.
    """
    Ta = linear_activation_step(a, a_initial, W, d)
    Tb = linear_activation_step(b, a_initial, W, d)
    lhs = float(np.sum(np.abs(Ta - Tb)))
    rhs = d * float(np.sum(np.abs(a - b)))
    return lhs, rhs
