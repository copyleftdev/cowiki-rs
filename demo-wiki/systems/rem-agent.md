# REM Agent

The background maintenance process for the Co-Wiki, named after REM sleep where biological [[cognitive/memory-consolidation]] occurs.

Three operators run on each maintenance cycle:

Decay: edge weights decrease exponentially with access recency.
w_t(i,j) = w_0(i,j) * exp(-lambda * (t - t_last(i)))
This models the Ebbinghaus forgetting curve. Unused associations weaken. Proven: decay is monotonically increasing with access recency (P6.1).

Prune: remove articles whose activation never exceeds a threshold over a sliding window. This is the wiki gardening problem solved automatically -- stale pages migrate to cold storage instead of cluttering warm storage.
Proven: active nodes are never pruned (P6.3). Dormant nodes are always prunable (P6.4).

Dream: discover missing backlinks between articles that are similar but not yet connected. Uses TF-IDF cosine similarity as the oracle.
Proven: dream never proposes duplicate edges (P6.6).

Health metric: H(G) = fraction of articles reachable from at least one probe query.
Proven: health stays bounded over maintenance cycles (P6.7).

The dream operator is the most interesting. It's finding connections you didn't know existed -- the same thing your brain does during REM sleep. See [[cognitive/memory-consolidation]] for the neuroscience, [[cognitive/priming]] for the mechanism.

Tested under [[distributed/fault-injection]]-style chaos: 22,500 operations including weight corruption, topology mutations, and rapid query alternation. All invariants held.
