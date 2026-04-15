# Knapsack Retrieval

Given activation scores and token costs, retrieval is a 0-1 knapsack problem: maximize total activation within a token budget.

The modified greedy algorithm guarantees at least half of optimal: result = max(greedy_by_density, best_single_item).

This allows the Co-Wiki to prefer small, highly relevant articles over large, moderately relevant ones. Variable chunk sizes from human-cognitive chunking create density trade-offs that fixed-token RAG cannot exploit.

See [[ai/spreading-activation]] for how activation scores are computed.
