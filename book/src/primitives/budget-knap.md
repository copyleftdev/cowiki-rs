# budget-knap

Modified-greedy 0/1 knapsack selection. Takes a scored item list
and a cost budget, returns the subset that maximizes total score
under the budget. Proven ≥½-OPT approximation; measured typical
ratio ≈ 1.00.

## Public API

```rust
use budget_knap::{select, Item, Selection};
```

### Types

```rust
pub struct Item {
    pub idx: usize,   // caller's opaque index
    pub score: f64,
    pub cost: u64,
}

pub struct Selection {
    pub items: Vec<usize>,  // values of Item::idx, input order preserved
    pub total_score: f64,
    pub total_cost: u64,
    pub converged: bool,    // always true; kept for API symmetry w/ spread
}
```

### Entry points

| signature | guarantee | use |
|---|---|---|
| `fn select(items: &[Item], budget: u64) -> Selection` | **≥½-OPT** | Production. Modified greedy: max of density-greedy and singleton-fit. |
| `fn greedy_by_density(items: &[Item], budget: u64) -> Selection` | unbounded in adversarial case | Tests only — the density-greedy arm in isolation. |
| `fn optimal_bruteforce(items: &[Item], budget: u64) -> Selection` | exact DP | Tests only — reference against which `select`'s ratio is measured. O(m · B) time. |

## Invariants

<div class="claim">

**½-OPT bound.** Let \\(S^*\\) be the optimal 0/1 knapsack
selection and \\(S\\) the modified-greedy selection. Then
\\(\texttt{score}(S) \ge \tfrac{1}{2} \texttt{score}(S^*)\\).

</div>

<div class="claim">

**Budget respected.** `select` always returns a `Selection`
whose `total_cost ≤ budget`, regardless of input. Empty selection
(`items == []`, `total_cost == 0`) is returned iff no single
item fits in the budget.

</div>

## The modified-greedy algorithm

```text
input:  items I, budget B

path A  — greedy by score/cost density:
    sort items by density descending
    take in order while remaining budget holds

path B  — singleton that fits alone:
    among items with cost ≤ B, take the one with max score

return  the higher-scoring of A and B
```

The singleton arm is what guarantees ½-OPT. Density-greedy alone
has adversarial cases that underperform by a factor proportional
to the item count; adding path B closes the bound at 2.

## Proof sketch

Sort items by density descending. Let \\(k\\) be the largest
index such that the first \\(k\\) items all fit; item \\(k+1\\)
is critical — it would be taken under fractional knapsack but
doesn't fit whole.

Fractional optimum: \\(L = s_1 + \cdots + s_k + \theta s_{k+1}\\)
for some \\(\theta \in [0, 1)\\). Since fractional ≥ integer,
\\(\texttt{score}(S^*) \le L\\).

- Path A takes at least \\(s_1 + \cdots + s_k\\).
- Path B picks an item of score ≥ \\(s_{k+1}\\) (since item
  \\(k+1\\) fits alone — it was the critical item).

\\[
\texttt{score}(\max(A, B))
\ge \frac{A + B}{2}
\ge \frac{s_1 + \cdots + s_k + s_{k+1}}{2}
\ge \frac{L}{2}
\ge \frac{\texttt{score}(S^*)}{2}. \quad \square
\\]

## Examples

```rust
use budget_knap::{select, Item};

let items = vec![
    Item { idx: 0, score: 5.0, cost: 3 },
    Item { idx: 1, score: 9.0, cost: 10 },
    Item { idx: 2, score: 4.0, cost: 2 },
    Item { idx: 3, score: 2.0, cost: 1 },
];
let result = select(&items, 10);
// result.items is a subset; total_cost ≤ 10; total_score ≥ ½ × DP-optimal.
```

## Measured ratio

Runtime audit on `game-theory` corpus (20 queries × 3 budgets):

| budget | min ratio to DP-optimal | median | max |
|---|---|---|---|
| 1000 | 0.9448 | 1.0000 | 1.0000 |
| 4000 | 0.9811 | 1.0000 | 1.0000 |
| 16000 | 1.0000 | 1.0000 | 1.0000 |

The ½-OPT bound exists for adversarial inputs; organic retrieval
almost never exercises it.

## Notes

<div class="aside">

**Cost must be positive.** A zero-cost item has infinite density
and would be selected unconditionally. `ScoredGraph::new`
enforces \\(c_i > 0\\) at graph construction for this reason.

</div>

<div class="aside">

**No diversity, no deduplication.** Given two items with nearly
identical scores and non-zero costs, both can land in the same
selection. Diversity-aware selection belongs at the caller; it's
not this crate's job.

</div>

## Proof obligations

- `half_opt_property` — proptest: generate random item sets and
  budgets, assert `select().total_score ≥ 0.5 × optimal_bruteforce().total_score`.
- `budget_respected` — across all proptest cases,
  `select().total_cost ≤ budget`.
- `edge_cases` — budget = 0, budget < cheapest item, budget ≫
  sum of all costs; all return sensible selections.

Runtime audit suite re-asserts the same properties against real
corpus activations on every run.
