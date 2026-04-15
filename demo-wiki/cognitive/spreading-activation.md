# Spreading Activation

A model of human memory retrieval from Collins and Loftus (1975). When you think of "doctor," related concepts like "nurse," "hospital," and "stethoscope" activate through associative links. Activation decays with distance.

This is the retrieval mechanism behind the Co-Wiki. A query activates seed nodes and propagation follows backlinks. The math is a contraction mapping: T(a) = (1-d)*a0 + d*W^T*a, which guarantees convergence to a unique fixed point.

Key insight from our verification work: the hard threshold function breaks contraction. Use a sigmoid instead. See [[ai/spreading-activation]] for the formal treatment.

Related cognitive processes:
- [[cognitive/priming]] uses the same associative network
- [[cognitive/memory-consolidation]] is what the [[systems/rem-agent]] biomimics
- [[cognitive/chunking]] affects how we organize the knowledge graph

The neuroscience parallel is remarkably tight. Hebbian learning ("neurons that fire together wire together") maps directly to backlink weight strengthening through repeated co-access.
