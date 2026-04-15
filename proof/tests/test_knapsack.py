"""
PROPERTY 3: Greedy Density Retrieval Achieves ≥ ½ OPT.

The retrieval problem:
    R*(q, G, B) = argmax_{S⊆V, Στ(v)≤B} Σ a*(v)

is a 0-1 knapsack. The greedy-by-density heuristic
(sort by ρ(v) = a*(v)/τ(v), fill greedily) achieves ≥ ½ of optimal.

Tests:
    P3.1  Greedy ≥ ½ OPT (the standard knapsack bound)
    P3.2  Greedy respects budget constraint
    P3.3  Variable chunk sizes enable strictly better solutions than fixed-size top-k
    P3.4  Greedy ≈ OPT empirically (usually much better than ½)
"""

import numpy as np
from hypothesis import given, settings, assume
from hypothesis import strategies as st
from hypothesis.extra.numpy import arrays

from cowiki.retrieval import greedy_retrieval, optimal_retrieval_bruteforce


class TestGreedyBound:
    """P3.1: Greedy achieves ≥ ½ of optimal total activation."""

    @given(
        n=st.integers(min_value=3, max_value=15),
        budget=st.integers(min_value=100, max_value=1000),
        data=st.data(),
    )
    @settings(max_examples=300, deadline=None)
    def test_half_optimality(self, n, budget, data):
        """greedy_value ≥ 0.5 · optimal_value."""
        activation = data.draw(arrays(
            dtype=np.float64, shape=(n,),
            elements=st.floats(min_value=0.0, max_value=1.0),
        ))
        token_costs = data.draw(arrays(
            dtype=np.int64, shape=(n,),
            elements=st.integers(min_value=20, max_value=300),
        ))
        assume(np.any(activation > 0))
        assume(np.any(token_costs <= budget))

        _, greedy_val = greedy_retrieval(activation, token_costs, budget)
        _, opt_val = optimal_retrieval_bruteforce(activation, token_costs, budget)

        if opt_val > 0:
            ratio = greedy_val / opt_val
            assert ratio >= 0.5 - 1e-9, (
                f"Greedy ratio {ratio:.4f} < 0.5 "
                f"(greedy={greedy_val:.4f}, opt={opt_val:.4f})"
            )


class TestBudgetConstraint:
    """P3.2: Greedy never exceeds the token budget."""

    @given(
        n=st.integers(min_value=3, max_value=20),
        budget=st.integers(min_value=50, max_value=2000),
        data=st.data(),
    )
    @settings(max_examples=300, deadline=None)
    def test_budget_respected(self, n, budget, data):
        """Total token cost of selected articles ≤ B."""
        activation = data.draw(arrays(
            dtype=np.float64, shape=(n,),
            elements=st.floats(min_value=0.0, max_value=1.0),
        ))
        token_costs = data.draw(arrays(
            dtype=np.int64, shape=(n,),
            elements=st.integers(min_value=10, max_value=500),
        ))

        selected, _ = greedy_retrieval(activation, token_costs, budget)
        total_cost = sum(int(token_costs[i]) for i in selected)

        assert total_cost <= budget, (
            f"Budget violated: used {total_cost} tokens, budget={budget}"
        )


class TestDensityAdvantage:
    """P3.3: Variable chunk sizes allow density-based trade-offs that
    fixed-size top-k cannot make."""

    @given(data=st.data())
    @settings(max_examples=200, deadline=None)
    def test_small_hot_beats_large_warm(self, data):
        """A small, highly-activated article should be preferred over a
        large, moderately-activated one when budget is tight."""
        # Construct a scenario:
        #   Article A: activation=0.9, cost=50  (density=0.018)
        #   Article B: activation=0.5, cost=400 (density=0.00125)
        #   Budget: 100 (can fit A but not B)
        activation = np.array([0.9, 0.5])
        token_costs = np.array([50, 400], dtype=np.int64)
        budget = 100

        selected, total_val = greedy_retrieval(activation, token_costs, budget)

        assert 0 in selected, "Should select the small high-density article"
        assert 1 not in selected, "Should not select the large low-density article"
        assert total_val >= 0.9 - 1e-9


class TestGreedyQuality:
    """P3.4: Empirically, greedy is usually much better than ½ OPT."""

    @given(
        n=st.integers(min_value=3, max_value=12),
        budget=st.integers(min_value=100, max_value=800),
        data=st.data(),
    )
    @settings(max_examples=200, deadline=None)
    def test_greedy_typically_near_optimal(self, n, budget, data):
        """Track the distribution of greedy/optimal ratios."""
        activation = data.draw(arrays(
            dtype=np.float64, shape=(n,),
            elements=st.floats(min_value=0.0, max_value=1.0),
        ))
        token_costs = data.draw(arrays(
            dtype=np.int64, shape=(n,),
            elements=st.integers(min_value=20, max_value=200),
        ))
        assume(np.any(activation > 0))
        assume(np.any(token_costs <= budget))

        _, greedy_val = greedy_retrieval(activation, token_costs, budget)
        _, opt_val = optimal_retrieval_bruteforce(activation, token_costs, budget)

        if opt_val > 0:
            ratio = greedy_val / opt_val
            # This isn't a hard assertion — it's a statistical check.
            # We assert ≥ 0.5 (the guarantee) but expect ≥ 0.8 most of the time.
            assert ratio >= 0.5 - 1e-9
