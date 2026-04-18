# budget-knap

`budget-knap` solves the 0/1 knapsack problem under the "modified
greedy" algorithm, with a proven \\(\tfrac{1}{2}\\)-OPT approximation
guarantee. It's the last step of the retrieval pipeline: given a
ranking (scored items) and a token budget, choose the subset whose
total score is as high as possible while respecting the cost cap.

The crate is ~200 lines. Short because the algorithm is short and the
proof is short.

## The problem

We have a set of items \\(I = \{1, \ldots, m\}\\), each with

- a **score** \\(s_i \ge 0\\) — the spreading-activation value,
- a **cost** \\(c_i > 0\\) — the token cost of including it.

And a **budget** \\(B \ge 0\\). We want to choose \\(S \subseteq I\\)
maximizing \\(\sum_{i \in S} s_i\\) subject to \\(\sum_{i \in S} c_i \le B\\).

This is the classic 0/1 knapsack. Exact solutions exist — dynamic
programming in \\(O(mB)\\), branch-and-bound in the worst case
exponential but fast in practice — but both scale badly when \\(m\\) or
\\(B\\) is large. In our use case, \\(m\\) is the number of candidate
documents from the spread (which is all of \\(n\\) in the worst case)
and \\(B\\) is a token budget in the thousands. At \\(n = 500{,}000\\)
the DP table would be \\(2.5 \times 10^9\\) cells. Not tractable at
query time.

We use the 2-approximation "modified greedy" instead.

## Modified greedy

```rust
pub fn select(items: &[Item], budget: u64) -> Selection {
    // Path A: greedy by score density (score / cost), descending,
    //         first-fit into budget.
    let dense = greedy_by_density(items, budget);

    // Path B: the single highest-score item that fits alone.
    let singleton = items.iter()
        .filter(|it| it.cost <= budget)
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(Equal));

    // Return whichever has higher total score.
    match singleton {
        Some(s) if s.score > dense.total_score => Selection { items: vec![s.idx], ... },
        _ => dense,
    }
}
```

Two passes over the items, take whichever scored more. The second
pass is the non-obvious part: sometimes the single most valuable item
that fits in the budget beats the density-greedy choice. Without this
tiebreak the approximation ratio degrades.

### Why density-greedy alone isn't enough

Consider an adversarial case. Budget \\(B = 10\\). Two items:

- Item 1: score 1, cost 1 (density 1)
- Item 2: score 9, cost 10 (density 0.9)

Density-greedy picks item 1 (higher density), spends 1 of the budget,
finds item 2 won't fit in the remaining 9, and stops. Total score: 1.
Optimal is item 2 alone. Score: 9. Ratio: \\(\tfrac{1}{9}\\).

Add the singleton comparison: path B picks item 2 (highest score that
fits alone), total 9. Path A picks item 1, total 1. We return 9.
Ratio to optimal: 1.

## The ½-OPT bound

<div class="claim">

**Claim.** Let \\(S^*\\) be the optimal 0/1 knapsack solution and let
\\(S\\) be the modified-greedy selection. Then
\\(\mathrm{score}(S) \ge \tfrac{1}{2} \mathrm{score}(S^*)\\).

</div>

**Proof.** Order items by density \\(s_i/c_i\\), descending. Let
\\(k\\) be the largest index such that the first \\(k\\) items all fit
in the budget; after that, item \\(k+1\\) is the "critical" item that
*would be added* if we were packing fractionally, but doesn't fit
whole.

The *fractional* knapsack, which allows splitting items, has value
\\(L = s_1 + s_2 + \cdots + s_k + \theta \cdot s_{k+1}\\) for some
\\(\theta \in [0, 1)\\). Since fractional knapsack is an upper bound
on the integer optimum, \\(\mathrm{score}(S^*) \le L\\).

Now bound each side of the modified-greedy decision:

- **Path A (density-greedy):** takes at least the first \\(k\\) items,
  so \\(\mathrm{score}(A) \ge s_1 + \cdots + s_k\\).
- **Path B (singleton):** if item \\(k+1\\) fits alone (that is,
  \\(c_{k+1} \le B\\), which holds because \\(k+1\\) was the critical
  item), singleton picks at least the best item of size \\(\le B\\),
  whose score is \\(\ge s_{k+1}\\).

So:

\\[
\mathrm{score}(\text{modified greedy}) = \max(A, B)
\ge \frac{A + B}{2}
\ge \frac{s_1 + \cdots + s_k + s_{k+1}}{2}
\ge \frac{L}{2}
\ge \frac{\mathrm{score}(S^*)}{2}. \quad \square
\\]

The factor of 2 comes from taking the max of two lower bounds. It's
tight: there exist instances where the ratio is exactly \\(\tfrac{1}{2}\\).

## Measured ratio

The bound is a worst case. In practice the algorithm does much
better. From the runtime audit suite on the `game-theory` corpus:

| budget | queries | min ratio to DP-optimal | median | max |
|---|---|---|---|---|
| 1000 | 20 | 0.9448 | 1.0000 | 1.0000 |
| 4000 | 20 | 0.9811 | 1.0000 | 1.0000 |
| 16000 | 20 | 1.0000 | 1.0000 | 1.0000 |

60 queries across three budget scales; the worst observed was
0.9448 (at a small budget where the density-greedy's early stop hurt
most). Typical is exactly optimal. The ½-OPT bound exists for the
adversarial worst case, but organic queries over organic corpora
almost never exercise it.

## The Item / Selection types

```rust
pub struct Item {
    pub idx: usize,   // original index into the source array
    pub score: f64,
    pub cost: u64,
}

pub struct Selection {
    pub items: Vec<usize>,  // indices (Item::idx), order preserved from input
    pub total_score: f64,
    pub total_cost: u64,
    pub converged: bool,    // always true here; kept for API symmetry w/ spread
}
```

`Item::idx` carries the *caller's* index so `Selection::items` can be
used to index back into whatever data structure produced the scores,
regardless of how `budget-knap` internally reorders items.

## The three entry points

- `select(items, budget)` — production path, modified greedy.
- `greedy_by_density(items, budget)` — pure path-A, exported for
  tests that need to compare the two paths independently.
- `optimal_bruteforce(items, budget)` — exponential DP reference.
  Exported specifically for the ratio tests in the runtime audit:
  we compute both `select` and `optimal_bruteforce` and assert the
  ratio ≥ 0.5. Don't call this in production.

## Cost model notes

Two practical points.

**Cost is integer.** Real token counts from the tokenizer are
integers; we keep `cost: u64` rather than allow floats. This means
the DP reference is tractable when we want it (cells are integer-
indexed), and it eliminates a class of floating-point comparison
headaches in the greedy.

**Cost must be positive.** A zero-cost item has infinite density and
would be selected unconditionally. `ScoredGraph::new` enforces
\\(c_i > 0\\) for exactly this reason. The invariant is checked at
graph construction; if a caller produces a zero-cost item, the graph
constructor panics rather than letting the knapsack return absurd
results.

## What the crate doesn't do

It doesn't filter by relevance, cluster-diversity, or recency. The
score is whatever the caller provided. If you want diversity-aware
selection or deduplication, that's the layer above — in cowiki-rs's
case, the `cowiki::retrieve` glue layer could add it in principle, but
the current composition doesn't.

The crate is one selection rule over a scalar ranking, proven ½-OPT,
measured ~1.0 in practice. That's the contract. Everything else is
the caller's job.
