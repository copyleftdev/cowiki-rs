# chunk-quality

Evaluation metrics. Not in the retrieval hot path — used by the
gauntlet suite, the runtime audit, and any consumer building
their own test fixtures.

## Public API

```rust
use chunk_quality::{
    recall, precision, f1,
    cosine_similarity,
    chunk_coherence, density_variance,
    hop_recall,
};
```

### Signatures

| signature | returns |
|---|---|
| `fn recall(retrieved: &[usize], relevant: &[usize]) -> f64` | \\(|R \cap T| / |T|\\); 0.0 if \\(T\\) empty |
| `fn precision(retrieved: &[usize], relevant: &[usize]) -> f64` | \\(|R \cap T| / |R|\\); 0.0 if \\(R\\) empty |
| `fn f1(retrieved: &[usize], relevant: &[usize]) -> f64` | harmonic mean of P, R |
| `fn cosine_similarity(a: &[f64], b: &[f64]) -> f64` | standard; 0 if either zero |
| `fn chunk_coherence(embeddings: &[Vec<f64>], boundaries: &[(usize, usize)]) -> f64` | mean pairwise cosine within each chunk |
| `fn density_variance(scores: &[f64], costs: &[u64]) -> f64` | variance of score/cost across items |
| `fn hop_recall(retrieved: &[usize], relevant_by_hop: &[(usize, Vec<usize>)]) -> Vec<(usize, f64)>` | recall stratified by graph distance |

All functions treat input slices as sets (duplicates deduped) and
handle zero-length inputs without panicking.

## Invariants

<div class="claim">

Each metric is bounded and well-defined:

- `recall`, `precision`, `f1` ∈ \\([0, 1]\\).
- `cosine_similarity` ∈ \\([-1, 1]\\).
- `f1(a, b) == f1(b, a)` — symmetric in retrieved ↔ relevant.
- `hop_recall` returns one entry per hop provided; each recall
  ∈ \\([0, 1]\\).

</div>

## Examples

### Standard triple

```rust
let retrieved = vec![10, 20, 30, 40];
let relevant  = vec![20, 40, 50];
assert_eq!(recall(&retrieved, &relevant),    2.0 / 3.0);
assert_eq!(precision(&retrieved, &relevant), 2.0 / 4.0);
assert!((f1(&retrieved, &relevant) - 0.5714).abs() < 0.01);
```

### Hop-stratified recall

```rust
let retrieved = vec![1, 2, 3, 4, 5];
let relevant_by_hop = vec![
    (0, vec![1]),           // the ignited node itself
    (1, vec![2, 3]),        // 1-hop neighbors
    (2, vec![4, 5, 6]),     // 2-hop neighbors
];
let by_hop = hop_recall(&retrieved, &relevant_by_hop);
// by_hop = [(0, 1.0), (1, 1.0), (2, 2/3)]
```

The 2-hop recall tells you whether spreading actually propagated
activation far enough to pick up distant relevant nodes. A
ranking with high overall recall but zero at hop ≥ 1 indicates
the graph isn't contributing — a regression worth investigating.

### Density variance as a diagnostic

```rust
let scores = vec![0.9, 0.8, 0.7, 0.1, 0.05];
let costs  = vec![100, 90, 80, 10, 5];
let var = density_variance(&scores, &costs);
// low var: items contribute similarly per token; healthy
// high var: a few runaway items dominate; may indicate budget too low
```

## What isn't here

- **NDCG, MRR, MAP.** Require meaningful ranking order. cowiki-rs
  returns selections (ordered lists exist but the order is
  informational, not contractual). Add to this crate if/when
  ranking order becomes part of a client contract.
- **Human-judgment infrastructure.** No labeling tool, no
  inter-annotator agreement. Ground-truth sets come from
  programmatic construction (gauntlet) or from small hand-labeled
  fixtures (runtime audit).
- **Significance testing.** The measurements in
  [Measurements](../measurements/end-to-end.md) are wall-clock
  timings and counts, not A/B comparisons requiring p-values.

## Reading guide

If you're new to information retrieval evaluation, reading
`crates/chunk-quality/src/lib.rs` top-to-bottom is a ~20-minute
introduction to the subset of metrics that actually matter for
this style of engine. Each function is a few lines with the
formula it's computing in a comment.

For a longer treatment, chapter 8 of Manning, Raghavan, and
Schütze (*Introduction to Information Retrieval*, 2008) is the
canonical reference. The seven metrics here are a subset
implemented with the same definitions.
