# Chunking

George Miller's "magical number seven" (1956): working memory holds 7 plus or minus 2 chunks. A chunk is a meaningful unit -- a phone number is 10 digits but 3 chunks (area code, prefix, line).

Expert chess players chunk entire board positions. Programmers chunk design patterns. The unit of comprehension is the chunk, not the byte.

This is why human-cognitive chunking beats fixed-token chunking for RAG:
- Humans naturally create article-sized chunks around complete ideas
- Fixed 512-token splits cut mid-thought
- Variable chunk sizes create density trade-offs that [[ai/knapsack-retrieval]] exploits

A small, focused article about one concept has higher activation density than a long survey article. The greedy retrieval preferentially selects the focused one -- which is the right behavior.

Related: [[cognitive/memory-consolidation]] (we consolidate chunks, not raw data), [[cognitive/spreading-activation]] (chunks are the nodes in the associative network).

The concept maps directly to wiki article size. When you create a wiki page, you're implicitly chunking at the boundaries of an idea achieving closure. That's the sweet spot.
