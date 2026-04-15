# Knapsack Retrieval

Given activation scores from [[ai/spreading-activation]] and token costs per article, retrieval is a 0-1 knapsack: maximize total activation within a token budget.

The modified greedy guarantees >= 1/2 of optimal:
result = max(greedy_by_density, best_single_item)

Verified against brute-force optimal across thousands of random instances.

Why variable chunk sizes matter:
- Fixed-token RAG collapses retrieval to trivial top-k (all chunks cost the same)
- Human-cognitive chunks (see [[cognitive/chunking]]) have variable sizes
- A 50-token article with 0.9 activation beats a 500-token article with 0.5
- The density rho = activation / tokens is the sorting key

This is the formal argument for why wiki articles (human-written, idea-sized) are better retrieval units than 512-token slices. The [[cognitive/chunking]] literature explains why humans naturally create good chunk boundaries.

The knapsack framing also applies to LLM context window management. When stuffing a prompt with context, you're solving the same problem: maximize relevance within a token budget. The [[ai/transformers]] attention mechanism then processes whatever you selected.
