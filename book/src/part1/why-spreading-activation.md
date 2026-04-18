# Why spreading activation

Search over a corpus has two jobs. The first is to accept a query and
return the documents closest to it under some similarity measure. The
second, which gets less attention, is to accept a query and return the
documents that *matter* given it — documents that do not share the
query's vocabulary but are central to its conceptual region.

These are different problems. The first is a nearest-neighbor problem
and is almost entirely solved. The second is a reachability problem
and isn't.

## The first job: similarity

Start with the familiar picture. A corpus has \\(n\\) documents. A query
arrives. The engine embeds both into some vector space, computes
cosine similarity between each document and the query, and returns the
top-\\(k\\).

This works. It works so well that the last decade of retrieval research
is mostly about making it work at scale: approximate nearest neighbor
indices (HNSW, IVF-PQ), distillation of encoders (BGE, Instructor),
late-interaction models (ColBERT). If the only question you need
answered is "which documents are most like this query," a modern
vector store with a decent encoder will answer it well enough that
most systems stop here.

## The second job: reach

Now change the question. Given a query that names a specific concept,
return the documents that are **central to the conceptual region that
concept lives in**.

A worked example. In a corpus of legal opinions, a query of
`"commerce clause"` run against cosine similarity returns pages whose
text contains the phrase "commerce clause" — Interstate Commerce
Commission v. *X*, *Y* v. Commerce Commission, and so on. That is what
the query asked for. It is not what the user wanted.

What the user wanted was Wickard v. Filburn. Wickard v. Filburn is the
1942 case that defined what "commerce" means for the purposes of the
clause. Its opinion does not contain the phrase "commerce clause"
often, and when it does, it's in passing. What the opinion *does* is
develop the reasoning that every subsequent commerce-clause case
references. Wickard is central to the concept without being a central
user of its vocabulary.

This is the reachability problem, and cosine similarity alone doesn't
solve it. The engine that solves it has to understand that the
conceptual region around `"commerce clause"` extends through the
citation graph — that a document two or three hops out from the
query's vocabulary hit, but heavily cited by those hits, is often more
important than the vocabulary hit itself.

## Spreading activation

Spreading activation is the retrieval algorithm that does this job
explicitly. It comes out of cognitive psychology, where it was
proposed in the 1970s as a model of how human semantic memory
retrieves related concepts from a prime. The ACT-R family of
cognitive architectures uses it. It has been reinvented several times
in information retrieval, each time as a response to the limits of
pure term-matching.

The mechanism is short enough to describe in one paragraph.

<div class="claim">

**Claim.** Given a directed graph \\(G = (V, E)\\) with edge weights
\\(W_{ij} \ge 0\\) row-normalized so \\(\sum_j W_{ij} = 1\\) for every
source \\(i\\), an initial activation vector \\(a^0 \in \mathbb{R}^n\\), a
threshold function \\(f : \mathbb{R} \to \mathbb{R}\\) with Lipschitz
constant \\(L \le 1\\), and a decay factor \\(d \in (0, 1)\\), the
iteration

\\[
a^{t+1} = d \cdot W^T f(a^t) + (1 - d) a^0
\\]

converges to a unique fixed point at rate \\(d \cdot L \le d\\). The
fixed point represents the stable activation of each node under
repeated propagation from the initial vector.

</div>

The interpretation is direct. Start with a query, compute its TF-IDF
activation vector across the corpus (the initial \\(a^0\\)). Propagate
that activation along edges of the graph, damped by \\(d\\) so
activation fades with distance, and push it through a non-linear
threshold \\(f\\) so only sufficiently-activated neighbors continue the
cascade. Iterate to convergence. The resulting activation vector is a
ranking over the corpus: nodes that were close to the query's
vocabulary get a direct boost from \\(a^0\\); nodes reachable by short,
high-weight paths from those get a mediated boost; nodes in
unreachable regions get nothing.

Two observations about this iteration that matter for the rest of the
book.

First, **the fixed point is unique** under the stated conditions. This
is a standard application of the Banach fixed-point theorem: the
iteration is a contraction with Lipschitz constant \\(d \cdot L < 1\\)
on the complete metric space \\((\mathbb{R}^n, \lVert \cdot \rVert_1)\\),
so there is exactly one attractor, and every starting point converges
to it at a geometric rate. We do not sample; we converge.

Second, **the conditions are load-bearing**. Row-stochasticity of
\\(W\\) is load-bearing: without it, total activation mass isn't
preserved across iterations and the ranking isn't comparable across
queries. \\(L \le 1\\) on the threshold is load-bearing: with a hard
threshold (Heaviside), the iteration can enter a limit cycle and
`converged` becomes a lie. \\(d < 1\\) is load-bearing: at \\(d = 1\\)
there is no contraction and the iteration orbits the graph's
spectrum forever. Each of these conditions breaks down in a different
direction, and each is a trap we have stepped in. They appear through
the book as constraints on the primitives, not as stylistic
preferences.

## Why not just combine cosine + PageRank?

A fair question. PageRank is also a fixed-point iteration over a row-
stochastic matrix. Could we pre-compute PageRank once per corpus, use
it as a prior, and combine it with per-query cosine similarity?

This is a real technique — it's roughly what query-biased PageRank and
topic-sensitive PageRank do. It works, but it pays two costs. The
first is that pre-computed PageRank is global: it tells you which
documents are central in general, not which are central *given this
query*. Combining it with cosine as a linear score improves rankings
on some queries and degrades them on others; there's no single
weighting that works across the query distribution. The second is
that the pre-computation must be refreshed whenever the graph
changes, and the graph changes with every new document.

Spreading activation avoids both costs. The activation at each node is
computed per-query from the query's own ignition vector, so relevance
is query-dependent by construction. And because the iteration runs on
demand, graph updates become visible on the next query; there is no
separate indexing step that could fall behind.

The cost we pay in return is per-query iteration cost. In practice,
for corpora up to the million-document range, this cost is ~5 ms
of sparse matrix-vector products; we document the measurements in
[Part V](../part5/end-to-end.md). That's a defensible trade given what
we get for it.

## The rest of this part

The remaining two chapters of Part I sharpen this picture by
contrasting it with the thing it's not.

[Chapter 2](what-vector-search-misses.md) lays out, with
a specific corpus, the kinds of queries where cosine similarity alone
returns the wrong answer and why spreading activation returns the
right one.

[Chapter 3](the-typed-graph-bet.md) makes the architectural commitment
explicit: the system is built around the assumption that you *have* a
typed graph — that every document in the corpus belongs to a
categorizable concept space and participates in explicit, authored
links. When that assumption is wrong, none of this works. We are
honest about that.
