# REM Agent

The REM Agent performs background maintenance on the Co-Wiki knowledge graph, inspired by memory consolidation during REM sleep.

Three operators drive the maintenance cycle:

Decay causes edge weights to decrease exponentially with access recency. Prune removes articles whose activation never exceeds a threshold over a sliding window, moving them to cold storage. Dream discovers missing backlinks between similar articles that are not yet connected, using TF-IDF cosine similarity as the oracle.

The REM Agent maintains graph health, preventing the wiki from fragmenting into unreachable islands.

See [[ai/spreading-activation]] for the retrieval mechanism that REM maintains.
