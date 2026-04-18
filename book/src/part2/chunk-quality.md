# chunk-quality

`chunk-quality` is the measurement crate. It doesn't participate in
the production retrieval path — it exists so the rest of the system
can be evaluated honestly.

Everything in it is a score: given some ground truth and some output
from the pipeline, return a number that says how close they are. The
crate is small (~150 lines) and each function fits in a screen.

## Why a whole crate for this

Two reasons.

First, **evaluation needs to be separable from the thing being
evaluated.** If `cowiki::retrieve` imported its own scoring functions,
it would be tempting to silently tune the pipeline against the scorer
in a way that broke external comparability. Having the scorer in a
separate crate with a narrow public API makes that harder to do
accidentally.

Second, **the same scorers get used in multiple places.** The
gauntlet tests score synthetic corpora. The runtime audit suite scores
real corpora. Proptests use `recall`, `precision`, and `f1` on random
inputs to catch degenerate cases. Consolidating them into one crate
means we don't have three slightly-different implementations of
`f1` to argue about later.

## The classic triple

```rust
pub fn recall(retrieved: &[usize], relevant: &[usize]) -> f64
pub fn precision(retrieved: &[usize], relevant: &[usize]) -> f64
pub fn f1(retrieved: &[usize], relevant: &[usize]) -> f64
```

The usual IR definitions, stated for clarity:

- \\(\texttt{recall} = |R \cap T| / |T|\\) — fraction of the truly
  relevant set \\(T\\) that was retrieved.
- \\(\texttt{precision} = |R \cap T| / |R|\\) — fraction of the
  retrieved set \\(R\\) that is actually relevant.
- \\(\texttt{f1} = 2 \cdot P \cdot R / (P + R)\\) — harmonic mean, the
  single number that drops when either P or R is low.

All three treat the input slices as sets; duplicates in either slice
are deduplicated before the intersection. All three return \\(0.0\\) on
empty inputs where a division by zero would otherwise occur.

The tests assert that `f1` is symmetric in swapping retrieved ↔
relevant, that `precision + recall = 1` whenever \\(R = T\\), and that
all three are bounded in \\([0, 1]\\).

## cosine_similarity

```rust
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64
```

Returns \\(\frac{a \cdot b}{\lVert a \rVert \cdot \lVert b \rVert}\\).
Symmetric. Returns 0 if either vector is zero. Used by the proptest
matrix to verify the spread iteration produces activation vectors
whose direction evolves smoothly under small perturbations of the
input.

The implementation is naive — three passes over the inputs (dot,
\\(a\\)-norm, \\(b\\)-norm). At the sizes used in tests (\\(n \le\\) a
few thousand) that's fine. If you need BLAS for longer vectors, use
BLAS directly; this crate is not trying to be fast.

## chunk_coherence

```rust
pub fn chunk_coherence(
    embeddings: &[Vec<f64>],
    boundaries: &[(usize, usize)],
) -> f64
```

A measure of how "internally consistent" each chunk is in an
embedding space. For each chunk (a contiguous slice of `embeddings`
bounded by a `(start, end)` pair), compute the average pairwise
cosine similarity between its members. Return the mean across
chunks.

Used to sanity-check chunking decisions. A coherent chunking partitions
the corpus so that members of each chunk are mostly about the same
thing; the score should be high. A bad chunking (random boundaries)
scatters unrelated items into the same chunks; the score drops.

Not used in the production cowiki-rs pipeline — there's no chunking
step in the current system — but retained because chunking is a
natural extension for larger documents, and having the metric ready
means we'd evaluate that extension honestly.

## density_variance

```rust
pub fn density_variance(scores: &[f64], costs: &[u64]) -> f64
```

Computes the variance of score-per-cost across items. A ranking with
tight density variance is one where every selected item contributes
similarly to the budget; a ranking with high variance has a few
runaway items dominating the density tail.

We use this in diagnostic tests. A spreading-activation result whose
density variance is abnormally high is usually a sign that the
knapsack has been forced to pick a few small high-score items
alongside a lot of large low-score items — sometimes a legitimate
ranking, sometimes a hint that the budget is too low for the corpus.

## hop_recall

```rust
pub fn hop_recall(
    retrieved: &[usize],
    relevant_by_hop: &[(usize, Vec<usize>)],
) -> Vec<(usize, f64)>
```

Given a retrieval and a per-hop breakdown of the relevant set,
return recall stratified by graph distance from the query's
ignition. `relevant_by_hop[k] = (k, docs)` means `docs` are the
documents at distance \\(k\\) from the query's ignited nodes.

This is the scorer that lets us verify that spreading activation
actually reaches distance-2 and distance-3 documents. A ranking
that achieves high overall recall but zero recall at distance ≥ 1 is
suspicious — it means the graph isn't contributing, and whatever
propagation was supposed to happen isn't happening.

`hop_recall` is how we catch regressions where a refactor of the
spread iteration accidentally reduces propagation. The VOPR tests
run it as a regression check after any mutation to `spread` or
`scored-graph`.

## What isn't in the crate

- **No ranking-aware metrics.** No NDCG, no MRR, no MAP. These are
  appropriate when the retrieved list has a meaningful order; the
  cowiki-rs pipeline treats the knapsack selection as an unordered
  set (the client receives a list, but the score ordering is
  informational, not contractual). If a future version promotes the
  ordering to a product contract, the ranking metrics should move
  into this crate.
- **No human-judgment infrastructure.** No tooling for eliciting
  relevance labels, no Cohen's kappa, no annotation UI. We make
  ground-truth sets by construction (programmatically in tests) or by
  hand, in small enough volumes that a labeling tool isn't
  worth building.
- **No significance testing.** The measurements we report in
  [Part V](../part5/end-to-end.md) are wall-clock timings and
  per-rung counts, not A/B comparisons that would need a p-value.

## The pedagogic value

A reader new to information retrieval will find `chunk-quality`
useful *independently* of cowiki-rs. Each of the seven functions is
a few lines, reads like prose, and documents the formula it's
computing. Reading them in order is a compressed introduction to
retrieval evaluation that takes maybe twenty minutes.

If you want a longer treatment, the canonical reference is still
chapter 8 of Manning, Raghavan, and Schütze (*Introduction to
Information Retrieval*, 2008). The seven functions here are a subset
of what's in that chapter, implemented with the same definitions and
none of the library-building overhead.
