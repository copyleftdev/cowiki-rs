# Transformers

The dominant neural network architecture since Vaswani et al. (2017). Self-attention lets the model attend to all positions in the input simultaneously, replacing the sequential processing of RNNs.

Core components:
- Self-attention: Q, K, V projections. Attention(Q,K,V) = softmax(QK^T/sqrt(d_k)) * V
- Multi-head attention: parallel attention heads capture different relationship types
- Feed-forward layers: position-wise nonlinearity
- Positional encoding: injecting sequence order (since attention is permutation-invariant)

The connection to [[ai/spreading-activation]] is deeper than analogy. Both compute weighted sums over a graph of associations. Attention weights are learned; spreading activation weights come from backlinks. Attention operates on token positions; activation operates on wiki articles.

The [[ai/attention-mechanisms]] page covers the mechanics in detail.

For retrieval, transformers produce the embeddings that vector-based RAG uses for similarity search. The Co-Wiki argument is that [[ai/knapsack-retrieval]] over a backlink graph beats embedding similarity for multi-hop associative queries.

Practical note: the TF-IDF ignition function in the Co-Wiki backend is a poor man's transformer embedding. A real deployment would use a transformer encoder for the initial activation vector, then let [[ai/spreading-activation]] propagate through the graph structure that embeddings can't capture.

See [[reading-notes/thinking-fast-and-slow]] for why fast associative retrieval (System 1) complements deliberate search (System 2).
