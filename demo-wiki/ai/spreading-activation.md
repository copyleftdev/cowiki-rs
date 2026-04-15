# Spreading Activation

Spreading activation is a network propagation algorithm inspired by cognitive science. Given a query, activation begins at seed nodes and spreads through weighted edges.

The linear operator is provably contracting: T(a) = (1 - d) * a0 + d * W^T * a

This means activation converges to a unique fixed point regardless of starting conditions. The convergence rate is geometric with base d.

See [[ai/transformers]] for a major application area, and [[ai/knapsack-retrieval]] for how we select articles from the activation vector.

The [[systems/rem-agent]] uses spreading activation to maintain graph health.
