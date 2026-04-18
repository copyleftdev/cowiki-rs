# What cowiki-rs is

A retrieval engine that answers the question "given a search query
over a corpus with an authored link graph, return the most valuable
reachable documents under a token budget."

Three-step pipeline, each step a separate proven primitive:

1. **Ignite.** Compute the initial activation vector \\(a^0\\) from
   the query's TF-IDF scores against every document. Cheap, O(query
   terms × average postings length).

2. **Spread.** Propagate activation along graph edges under the
   iteration \\(a^{t+1} = d \cdot W^\top f(a^t) + (1-d) a^0\\) to a
   fixed point. Non-linear threshold \\(f\\) with Lipschitz constant
   \\(L \le 1\\), damping \\(d < 1\\), so \\(d \cdot L < 1\\) and the
   iteration is a contraction. Typically converges in 20–30
   iterations.

3. **Select.** Run a 0/1 knapsack over the resulting activation
   vector and per-document token costs, returning the subset that
   maximizes total activation under the token budget. Modified
   greedy with a proven ≥½-OPT bound; typical observed ratio to
   exact DP optimum is 1.00.

The engine does **not** do embedding generation, vector search,
reranking, or query parsing beyond tokenization. It expects a
graph as input. The graph can come from wiki backlinks, citation
networks, authored hyperlinks, or any other source where the edges
represent intentional inter-document relevance.

## Design bets

<div class="claim">

**Primary.** Relevance signal from authored edges outperforms
inferred similarity whenever the corpus is dense enough in edges
that reachability is informative — roughly, edge density above
\\(10^{-4}\\) of the complete graph.

</div>

<div class="claim">

**Secondary.** A layered architecture of small, independently-
proven primitives is more legible — and more correctable — than a
single optimizing black box, at a cost of ~15 ms per query versus
a hypothetical monolith's ~5 ms.

</div>

If those bets aren't bets you want to make for your corpus, other
retrieval systems are a better fit. Pure vector search (Qdrant,
Milvus, LanceDB) wins on unstructured corpora. Dedicated legal
search (Westlaw, Lexis) wins on legal corpora when you have
license access.

## When cowiki-rs is the right choice

- Your corpus has explicit inter-document links and the links
  encode relevance (wiki, citation graph, internal docs with
  systematic cross-references).
- You need the ranking to be explainable — every result's score is
  traceable to a short path from the query's vocabulary hits.
- You want a single binary with no managed-service dependency.
- You are comfortable at corpora from ~100 pages to ~500k pages on
  a single process; beyond that, you'll need the segment-sharding
  work described in [Scale envelope](../measurements/scale-envelope.md).

## When to look elsewhere

- Your corpus is raw text without links and you have no way to
  extract a graph. A vector store will outperform cowiki-rs on
  this.
- You need BM25 specifically. cowiki-rs uses TF-IDF for ignition
  because it's cheap and adequate for the ignition step; BM25
  would be a drop-in replacement but isn't implemented.
- You want per-query sub-millisecond latency on million-document
  corpora. cowiki-rs converges in 20–30 iterations of sparse
  matrix-vector products; at million-node scale that's tens of
  milliseconds.
- Your graph is adversarial (link-spam, citation rings,
  maintainer-controlled with no editorial review). The engine's
  ranking is manipulable in direct proportion to graph
  manipulation.

## The SCOTUS Explorer

The production instance at <https://scotus.cowiki.tech> demonstrates
what a well-tuned deployment looks like: 10,000 opinions, 133k
citation edges, p50 query latency 3.3 ms, live since mid-2026.
The full story of how that corpus was built, broken, and fixed is
in [Case Study: SCOTUS Explorer](../case-study/premise.md) — the
one narrative section of this documentation.
