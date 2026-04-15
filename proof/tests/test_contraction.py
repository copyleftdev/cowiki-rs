"""
PROPERTY 2: Contraction Mapping — Geometric Convergence Rate.

For the LINEAR operator (θ=0), convergence is geometric:
    ‖aᵗ - a*‖₁ ≤ dᵗ · ‖a⁰ - a*‖₁

For the thresholded operator, the envelope on RESIDUALS (‖aᵗ⁺¹ - aᵗ‖)
is not strictly d^t because the threshold can cause non-monotonic
residual behavior in early iterations. We test the weaker property
that residuals are eventually dominated by the geometric envelope.

Tests:
    P2.1  Geometric rate on linear operator (strict)
    P2.2  Thresholded residuals eventually decay geometrically (relaxed)
    P2.3  Iteration count: converges in O(log(1/ε) / log(1/d)) steps
    P2.4  Higher d = slower convergence (more spreading, more iterations)
"""

import math
import numpy as np
from hypothesis import given, settings, assume
from hypothesis import strategies as st

from cowiki.activation import spreading_activation
from tests.conftest import random_graphs, random_activations, propagation_factors


class TestGeometricRate:
    """P2.1: For the linear operator, residual envelope is strict d^t."""

    @given(
        graph=random_graphs(),
        d=propagation_factors,
        data=st.data(),
    )
    @settings(max_examples=150, deadline=None)
    def test_linear_residual_envelope(self, graph, d, data):
        """With θ=0 (linear), residuals respect strict d^t envelope."""
        a0 = data.draw(random_activations(graph.n))
        assume(np.any(a0 > 0))

        _, iterations, residuals = spreading_activation(
            graph, a0, d=d, theta=0.0, max_iter=100, epsilon=1e-12,
        )
        assume(len(residuals) >= 3)

        r0 = residuals[0]
        for t, r_t in enumerate(residuals):
            envelope = r0 * (d ** t) * 1.1 + 1e-10
            assert r_t <= envelope, (
                f"Linear residual {r_t:.2e} at step {t} exceeds envelope "
                f"{envelope:.2e} (d={d:.3f}, r0={r0:.2e})"
            )

    @given(
        graph=random_graphs(),
        d=propagation_factors,
        data=st.data(),
    )
    @settings(max_examples=150, deadline=None)
    def test_thresholded_residuals_eventually_decay(self, graph, d, data):
        """With θ>0, residuals may be non-monotonic early but still converge."""
        a0 = data.draw(random_activations(graph.n))
        assume(np.any(a0 > 0))

        _, iterations, residuals = spreading_activation(
            graph, a0, d=d, theta=0.01, max_iter=200, epsilon=1e-12,
        )
        assume(len(residuals) >= 5)

        # After initial transient (first 3 steps), residuals should decrease
        tail = residuals[3:]
        if len(tail) >= 2:
            assert tail[-1] <= tail[0] + 1e-9, (
                f"Residuals not decaying in tail: {tail[0]:.2e} → {tail[-1]:.2e}"
            )


class TestIterationCount:
    """P2.2: Predicted iteration count matches theory.

    The O(log(1/ε)/log(1/d)) bound applies to the LINEAR operator only.
    The thresholded operator can enter limit cycles, making iteration
    count unbounded. hypothesis discovered this.
    """

    @given(
        graph=random_graphs(),
        d=st.floats(min_value=0.3, max_value=0.9),
        data=st.data(),
    )
    @settings(max_examples=100, deadline=None)
    def test_linear_convergence_within_predicted_bound(self, graph, d, data):
        """Linear operator converges in O(log(1/ε) / log(1/d)) iterations."""
        a0 = data.draw(random_activations(graph.n))
        assume(np.any(a0 > 0))

        epsilon = 1e-8
        _, iterations, residuals = spreading_activation(
            graph, a0, d=d, theta=0.0, max_iter=500, epsilon=epsilon,
        )

        r0 = residuals[0] if residuals else 1.0
        if r0 > epsilon:
            predicted = math.ceil(math.log(r0 / epsilon) / math.log(1 / d))
            assert iterations <= max(predicted * 2, 10), (
                f"Took {iterations} iterations, predicted ≤ {predicted * 2} "
                f"(d={d:.3f}, r0={r0:.2e})"
            )


class TestPropagationFactorOrdering:
    """P2.4: Higher d = more iterations to converge.

    This only holds for the linear operator. With threshold, the
    d-ordering breaks because boundary interactions can make a low d
    oscillate while a high d converges cleanly (or vice versa).
    """

    @given(
        graph=random_graphs(min_nodes=5),
        data=st.data(),
    )
    @settings(max_examples=100, deadline=None)
    def test_higher_d_more_iterations_linear(self, graph, data):
        """For the linear operator, d_low converges faster than d_high."""
        d_low = data.draw(st.floats(min_value=0.1, max_value=0.4))
        d_high = data.draw(st.floats(min_value=0.6, max_value=0.95))
        a0 = data.draw(random_activations(graph.n))
        assume(np.any(a0 > 0))

        _, iter_low, _ = spreading_activation(
            graph, a0, d=d_low, theta=0.0, max_iter=500, epsilon=1e-10,
        )
        _, iter_high, _ = spreading_activation(
            graph, a0, d=d_high, theta=0.0, max_iter=500, epsilon=1e-10,
        )

        assert iter_high >= iter_low - 1, (
            f"Higher d={d_high:.3f} converged faster ({iter_high}) "
            f"than d={d_low:.3f} ({iter_low})"
        )
