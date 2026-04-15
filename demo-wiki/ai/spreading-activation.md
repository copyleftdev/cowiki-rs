# Spreading Activation (Formal)

The mathematical formalization of [[cognitive/spreading-activation]] for the Co-Wiki retrieval engine.

Two operator variants were defined and verified:

Linear (provably contracting):
T_lin(a) = (1-d) * a0 + d * W^T * a
Contraction: ||T(a)-T(b)|| <= d * ||a-b||

Thresholded (convergent in practice, can limit-cycle):
T(a) = (1-d) * a0 + d * W^T * f(a)

The verification suite found three corrections to the initial model:
1. Hard threshold breaks contraction (not Lipschitz-1)
2. Hard threshold can cause limit cycles
3. Monotonic hop-decay only holds on pure chains

All three were corrected. The sigmoid threshold restores all guarantees.

The operator is structurally identical to PageRank with a personalization vector. The (1-d)*a0 term anchors activation to the query; d controls how far activation drifts into the graph.

Applications beyond wiki retrieval:
- [[security/attack-surface-mapping]] uses the same graph traversal pattern
- [[security/threat-modeling]] can model threat propagation as activation spread
- [[cognitive/priming]] is the biological version

See [[ai/knapsack-retrieval]] for how activated nodes are selected under a budget, and [[ai/transformers]] for the attention mechanism parallel.
