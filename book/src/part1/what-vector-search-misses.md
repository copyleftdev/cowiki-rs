# What vector search misses

This chapter argues by example. We'll walk through four query types
where a well-tuned vector search returns the wrong answer, say
*precisely* why it does, and describe what the spreading-activation
pipeline returns instead.

Our test corpus throughout is `scotus-top10k` — the ten thousand
most-cited Supreme Court opinions (see [Part IV](../part4/premise.md)
for how we built it). Baseline is a dense encoder (`all-MiniLM-L6-v2`
embeddings over the opinion body text, cosine similarity, top-\\(k\\)).
Counterexample is the cowiki-rs pipeline: TF-IDF ignition, spreading
activation over the citation graph with sigmoid threshold, knapsack
selection under a 6,000-token budget.

## Case 1. The central case is not a vocabulary match

**Query:** `"commerce clause interstate regulation"`

**What cosine returns.** Cases whose body text contains the phrase
"commerce clause" prominently — *Illinois Central Railroad v.
Behrens*, *Pennsylvania v. Interstate Commerce Commission*,
*United Shoe Machinery Corp. v. United States*. Correct by the measure
it's optimizing, and useful if what you want is pages that discuss the
clause by name.

**What it misses.** *Wickard v. Filburn*. *Gibbons v. Ogden*. *Lopez
v. United States*. *Heart of Atlanta Motel*. These are the four
opinions that most students of American constitutional law would name
as central to the Commerce Clause doctrine. Their texts use the phrase
less than the discussion cases do; the doctrine is developed by
analyzing an activity (wheat-growing, navigation, gun possession,
motel accommodations) and concluding about its relation to commerce.
Doctrinal weight sits in the reasoning, not the label.

**What spreading activation returns.** The same initial vector — TF-
IDF over the query terms — ignites many of the same documents as
cosine at \\(t=0\\). The difference happens on iterations 1 through ~30.
Activation flows along the citation graph. Each of the vocabulary-hit
cases cites one or more of the landmark cases heavily; Wickard, in
particular, is cited by ~800 commerce-clause cases in the corpus. The
landmark's activation on iteration 2 is the sum of the inbound flows
from those vocabulary hits, scaled by edge weight and damping factor.
By convergence, Wickard's score has overtaken the pages that merely
*discussed* commerce.

The graph did the work. The algorithm never parsed the prose; it read
who cites whom, and that was enough.

## Case 2. The query is abstract, the corpus is particular

**Query:** `"due process substantive"`

**What cosine returns.** Cases that explicitly name "substantive due
process" — mostly modern cases (post-1970) where the doctrine is
discussed in its current, named form. Accurate and modern, but
historically hollow.

**What it misses.** *Lochner v. New York* (1905). *Meyer v. Nebraska*
(1923). *Griswold v. Connecticut* (1965). The precedents that *built*
substantive due process before anyone called it that. Lochner in
particular is the case that gave the doctrine its name (the "Lochner
era"), but the opinion itself does not contain the phrase; it works
the idea out in the language of liberty-of-contract.

**Why.** Cosine similarity is a term-matching technique under a
coordinate transform. Even a well-trained encoder learns synonymy
from co-occurrence in training data, not from citation structure. It
has no way of knowing that Meyer and Griswold *built* the region that
modern cases now *label*.

**What spreading activation returns.** TF-IDF ignites on "due
process" and "substantive" in modern cases. Those modern cases cite
Griswold (because Griswold is the modern lineage point). Griswold
cites Meyer and Lochner (because that's the historical lineage). Two
hops out from ignition, Lochner has accumulated activation from a
dozen ancestral paths through the citation graph. It's in the top-20
of the final ranking.

The corpus is particular — specific cases, specific courts — but the
query is abstract. The citation graph is what bridges them.

## Case 3. The vocabulary collision

**Query:** `"equal protection"`

**What cosine returns, verbatim from our pre-enrichment corpus:**

```
  Environmental Protection Agency v. Maryland
  Rosenberg v. Federal Protection Service
  Nebraska v. Environmental Protection Agency
```

These are not equal-protection cases. They are administrative-law
cases about the Environmental Protection Agency and the Federal
Protection Service. The encoder learned "protection" as a coherent
unit of meaning from training data that is not specific to
constitutional law; it collides on vocabulary.

<div class="postmortem">

**Postmortem.** We shipped the SCOTUS Explorer's first stub corpus
with these results visible on the landing page. A stranger using the
product would reasonably conclude that the Equal Protection Clause
doesn't apply to schools, police, segregation, or anything a law
student would recognize — because those cases were 20 ranks down from
the EPA cases on the first page.

The fix wasn't a better encoder. The fix was that the stub corpus had
no body text, so there was nothing *but* the title for TF-IDF and the
encoder to match against. Once we enriched the pages with the
opinions' own prose and wired the citation graph, "equal protection"
started returning Brown v. Board of Education, Davis v. Board of
School Commissioners, Loving v. Virginia.

The algorithm was not wrong before enrichment. It was answering the
question the data allowed it to answer. Our mistake was assuming a
corpus of ten thousand pages was enough data when the pages
themselves had nothing in them.

See [Part IV, Chapter 2](../part4/stubs.md) for the full story.

</div>

## Case 4. Query-dependent centrality

**Query A:** `"federalism"` (abstract constitutional principle)

**Query B:** `"ICBM treaty"` (specific Cold War policy instrument)

The corpus is exactly the same in both. Pure PageRank on the citation
graph — a query-independent centrality — produces one list of "the
most central cases," and it's the same list no matter what you asked.
That list features the 1986 procedural workhorses (*Anderson v.
Liberty Lobby*, *Celotex v. Catrett*) because those are the procedural
precedents every subsequent case cites.

Query A ("federalism") should surface *McCulloch v. Maryland* (1819)
near the top. It does: McCulloch is the foundational federalism case
and has ~162 citations in the corpus.

Query B ("ICBM treaty") should not surface *McCulloch v. Maryland*
at all. It shouldn't surface procedural workhorses either. It
should surface whatever Cold War era opinions discuss nuclear
weapons policy (and there aren't many — SCOTUS rarely adjudicates
treaty instruments directly).

Spreading activation, because the initial vector is *query-dependent*,
gives different rankings for A and B over the same graph. McCulloch
is activated by A's vocabulary (it's a federalism case by name) and
by the modern federalism cases that cite it. Under B's vocabulary,
McCulloch receives no direct ignition, and none of the modern
federalism cases that cite it got ignited either, so McCulloch's
neighborhood stays quiet. The ranking for B is specific to B's
corner of the graph.

Static centrality cannot do this. Per-query cosine cannot do this
alone; it doesn't consult the graph. The combination of ignition-
from-query plus propagation-through-graph is the mechanism.

## A way to summarize

The four cases share a structure.

- **Case 1** — the answer is at graph distance ≥ 1 from the query's
  vocabulary.
- **Case 2** — the answer predates the vocabulary.
- **Case 3** — the query's vocabulary is promiscuous; the graph is
  not.
- **Case 4** — the answer depends on *which* query; a static index
  cannot differentiate.

In each case, the signal that resolves the query correctly is *in the
graph*, not in the bag of words or the dense embedding. An engine that
doesn't consult the graph is flying on instruments that can't see it.

This is the job spreading activation does. It is not the only job of a
retrieval system — cosine similarity over good embeddings still beats
it for short factual lookups and for documents written in a
vocabulary close to the query's. cowiki-rs uses TF-IDF for ignition
precisely because it's cheap and adequate for that job, and saves the
iteration budget for the reachability problem that ignition alone
can't solve.

## What this costs

Three things.

**An explicit graph.** Not an inferred similarity graph (which
collapses to the vector search we just argued wasn't enough), but
authored links — the wiki backlink, the legal citation, the academic
reference. Corpora without this structure cannot use cowiki-rs
productively; the graph carries all the signal that vocabulary
doesn't. The next chapter is about this commitment and what it rules
out.

**Per-query iteration time.** Five to thirty iterations of a sparse
matrix-vector product, where the sparsity is in the graph and the
density is in the activation vector. For the million-node regime this
is a few milliseconds; beyond that, the techniques in
[Part V](../part5/scale-envelope.md) push the envelope further.

**Explainability.** We consider this a gain, not a cost. Every
ranking decision in a cowiki-rs query is a sum of edge contributions
along a short path from ignition. You can literally trace why a given
document ranks where it does, edge by edge. No attention visualization
over an encoder makes that claim.
