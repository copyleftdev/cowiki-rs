# Attention Mechanisms

Attention allows models to focus on relevant parts of the input, weighted by learned compatibility scores.

Scaled dot-product attention:
Attention(Q, K, V) = softmax(QK^T / sqrt(d_k)) * V

Variants:
- Self-attention: Q, K, V come from the same sequence
- Cross-attention: Q from one sequence, K/V from another
- Causal attention: masked to prevent attending to future positions
- Flash attention: IO-aware exact attention (Dao et al., 2022)

The structural parallel to [[cognitive/spreading-activation]] is exact: attention computes a weighted sum over a set of values, where weights are determined by compatibility between the query and the keys. Spreading activation computes a weighted sum over the activation vector, where weights are determined by the graph adjacency.

The difference: attention weights are dynamic (computed per input), while graph weights are structural (fixed by backlinks). This is why the Co-Wiki uses both: [[ai/transformers]] for initial query understanding, then [[ai/spreading-activation]] for structural traversal.

Central to [[ai/transformers]]. Related to [[cognitive/priming]] (attention as a computational model of selective priming).
