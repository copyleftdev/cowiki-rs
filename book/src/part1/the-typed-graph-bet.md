# The typed-graph bet

Every architecture is a bet. cowiki-rs is a bet that you have — or can
produce — a graph whose edges are authored, typed, and trustworthy.

This chapter describes what that bet means, what it rules out, and
how we hedge it.

## The bet, stated plainly

<div class="claim">

**Claim.** A retrieval system whose relevance signal comes from
*explicit, authored, directed edges* between documents will outperform
one whose signal comes from *inferred similarity* whenever the corpus
is dense enough in edges that reachability is informative.

</div>

"Dense enough" is not precise. Part V gives a quantitative bound
(Section [Scale envelope](../part5/scale-envelope.md)) — empirically,
edge density below about \\(10^{-4}\\) of the complete graph starts
producing spreading-activation rankings that approximate pure TF-IDF,
because activation has nowhere to spread to.

What matters for this chapter is the qualitative shape: the bet pays
off when the graph carries structure that the document texts don't,
and it loses when it doesn't.

## What "authored" means

An edge \\((u, v)\\) is *authored* when some agent — a human writer, a
court's opinion, a wiki editor, a paper's bibliography — explicitly
links document \\(u\\) to document \\(v\\). The author stands behind the
link. They asserted that \\(v\\) is relevant to \\(u\\), in the context of
\\(u\\).

Examples that qualify:

- A Wikipedia page linking to another page via `[[target|display]]`.
- A legal opinion citing a previous opinion as precedent.
- An academic paper including another paper in its references.
- An internal wiki page about "Q3 roadmap" that explicitly links to
  the page about "H2 customer research."

Examples that don't:

- A cosine-similarity edge inferred from embeddings. Nobody authored
  it; the encoder inferred it. It's only as reliable as the encoder's
  training distribution.
- A k-NN edge over TF-IDF vectors. Same problem.
- An edge created by keyword co-occurrence. An artifact of
  vocabulary.

The distinction is subtle but load-bearing. Authored edges encode
*intentional* relevance; inferred edges encode *statistical*
similarity. Spreading activation over authored edges produces
rankings whose provenance you can explain. Spreading activation over
inferred edges produces rankings that are laundering the encoder's
biases through a fixed-point iteration.

## What "typed" means

An edge type is a categorical label on the edge that distinguishes
*kinds* of links. In the SCOTUS corpus, we use edge weights rather
than explicit types — all edges are citation edges, weighted by
citation depth. In the base engine, however, `temporal-graph` tracks
three distinct kinds of participation for each edge (live, decayed,
dreamed), and the `ScoredGraph` primitive supports per-node category
tags that could be promoted to edge-level typing in a future corpus.

For this book's purposes, "typed" means: *the graph's semantics is
rich enough that different edges carry different weight in the
activation flow*. A citation with depth ×5 propagates more activation
than a citation with depth ×1. A primary reference propagates more
than a see-also. A reciprocal link propagates differently than a one-
way link.

The bet includes the assumption that you have access to this
information. If all you have is a raw link graph — no weights, no
types, no distinction between important and incidental links — the
engine can still work, but it's working with less information than
it wants.

## What "trustworthy" means

The third condition is that the edges are not adversarial. The
engine treats authored edges as ground truth for relevance; if those
edges are manipulated (link-spam, citation rings, editorial vandalism)
the ranking is manipulable in direct proportion.

This is the same trust model as the original PageRank, and it has the
same failure modes. Wikipedia has editorial processes that keep
spammy-edges rare. CourtListener's legal citation graph is naturally
resistant: courts aren't citing cases for SEO. Enterprise wikis with
healthy editorial culture are usually fine; wikis that have become
dumping grounds for stale content are not.

<div class="aside">

**Aside.** The rarely-discussed cost of the "trustworthy" assumption
is that it bounds the corpora we're willing to take on. A Reddit-
scraped corpus would not satisfy it. An LLM-generated corpus almost
certainly would not. We haven't tried to deploy cowiki-rs against a
hostile graph and we don't have confidence in how it would degrade.

</div>

## What the bet rules out

Being explicit about this:

- **Unstructured text corpora.** A directory of PDF technical papers
  with no extracted citation graph is not a good fit, even if the
  papers do cite each other, because the engine has no way to see
  those citations unless someone produces the graph. You can extract
  a citation graph from papers (see the [enrichment
  chapter](../part4/enrichment.md) for how we did this with SCOTUS
  opinions) but it's its own project.

- **Conversational corpora.** Slack messages, email, chat transcripts
  — these usually don't have explicit inter-message links. A threading
  graph isn't a relevance graph; two messages in the same thread are
  adjacent by timing, not by authored reference.

- **Any corpus where the relevance signal is in the vocabulary, not
  the structure.** Short-form content (tweets, product reviews,
  customer support tickets) usually has no graph worth building over.
  A BM25 or dense-encoder retrieval system will outperform cowiki-rs
  on these by a wide margin.

## How we hedge

Two hedges, both visible in the crate structure.

### Hedge 1: TF-IDF as the ignition path

The system doesn't rely on the graph *alone*. Every query begins with
a TF-IDF pass over the corpus that produces the initial activation
vector \\(a^0\\). If the query's vocabulary is a strong signal — if the
answer really is the document whose text most matches the query —
that document receives the largest initial activation, and the
subsequent spreading iterations boost related documents from there.

This matters because it means cowiki-rs does not underperform on
the queries where vector search does well. It still does term
matching, via the ignition step. The graph contributes marginal
value on top, and the graph's value grows as the graph's density
grows. On a perfectly disconnected corpus (edge density zero),
cowiki-rs reduces exactly to TF-IDF.

### Hedge 2: The engine is layered, not monolithic

The crate structure (see [Part II](../part2/scored-graph.md)) makes
the graph-dependent components swappable. The `scored-graph` crate is
the graph. The `spread` crate is the iteration. The `tfidf` module in
`wiki-backend` is the ignition. A consumer can substitute any of
these — use dense embeddings for ignition, use a different
propagation model, bolt on a different graph — without touching the
glue layer.

This isn't stated out loud anywhere in the code; it's a
consequence of how we drew the crate boundaries. If you find
yourself wanting to replace, say, the TF-IDF ignition with a dense-
vector ignition, the change is ~30 lines of code at the boundary and
nothing else moves.

## What remains

If you've accepted this far — that the engine bets on an authored,
typed, trustworthy graph, that the bet is hedged but not eliminated,
and that some corpora are outside its addressable market — the rest
of the book is about how the bet is cashed in.

[Part II](../part2/scored-graph.md) describes the primitives that make
the iteration work: the graph representation, the contraction
argument for convergence, the knapsack selection, the temporal
(REM-inspired) decay/prune/dream cycle. These are the five crates
the rest of the system is built on, each with an invariant that has
to hold for the composition above them to make sense.

[Part III](../part3/cowiki.md) describes the thin glue layer that
composes those primitives into a working retrieval pipeline, and the
backend + server that turn that pipeline into a process that talks
HTTP.

[Part IV](../part4/premise.md) documents the first real production
corpus — what broke, what we fixed, and what we would do differently
given a second pass. It is the part of the book most likely to be
useful to someone building a cowiki-rs deployment against their own
corpus.

[Part V](../part5/end-to-end.md) gives the measurements. Every claim
this chapter has made about performance — "a few milliseconds for
million-node regimes," "density below \\(10^{-4}\\) is a cliff," "per-
query iteration cost" — is quantified there with experimental setup.
