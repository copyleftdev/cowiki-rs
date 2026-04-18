# spread

`spread` is the iteration. It takes an initial activation vector — a
query's TF-IDF ignition, usually — and propagates it through the
graph according to the equation stated in
[Chapter 1](../part1/why-spreading-activation.md):

\\[
a^{t+1} = d \cdot W^\top f(a^t) + (1 - d) a^0
\\]

The crate is ~300 lines. Most of the interesting content is in the
threshold trait and the contraction proof.

## The threshold trait

```rust
pub trait ThresholdFn {
    fn apply(&self, x: f64) -> f64;
    fn lipschitz(&self) -> f64;
}
```

A threshold function decides *how much* of a node's current activation
is available to propagate to its neighbors on the next iteration. The
trait requires the implementor to declare its Lipschitz constant —
an upper bound on how sensitive the output is to changes in the
input:

\\[
|f(x) - f(y)| \le L \cdot |x - y| \quad \forall x, y \in \mathbb{R}
\\]

This is load-bearing. The convergence proof needs \\(L \le 1\\). If
\\(L > 1\\), the iteration is a *dilation*, not a contraction; small
perturbations grow, and repeated application can produce chaotic
dynamics rather than a fixed point.

Three implementations in the crate:

### NoThreshold

```rust
pub struct NoThreshold;
impl ThresholdFn for NoThreshold {
    fn apply(&self, x: f64) -> f64 { x }
    fn lipschitz(&self) -> f64 { 1.0 }
}
```

The identity function. \\(L = 1\\) exactly. Convergence holds. Useful
for pure PageRank-style propagation, or when you want no non-linearity
and trust the damping factor alone to bound the spread.

### SigmoidThreshold (the default)

```rust
pub struct SigmoidThreshold {
    center: f64,
    steepness: f64,
}
impl ThresholdFn for SigmoidThreshold {
    fn apply(&self, x: f64) -> f64 {
        1.0 / (1.0 + (-self.steepness * (x - self.center)).exp())
    }
    fn lipschitz(&self) -> f64 {
        self.steepness * 0.25   // max derivative of sigmoid at x=center
    }
}
```

Smooth, non-linear, bounded output in \\((0, 1)\\). The maximum slope of
a standard logistic is \\(0.25\\) at its center, so with `steepness=4`
the Lipschitz bound is exactly 1, and anything less is safe. The
default `SigmoidThreshold::default()` uses `center=0.1, steepness=2.0`,
giving \\(L = 0.5\\), well below the contraction threshold.

This is the threshold cowiki-rs uses in production. The sigmoid gives
the intuitive dynamic we want — small activations stay small, large
activations saturate near 1, and the transition is continuous.

### HardThreshold — why it's in the crate but not the default

```rust
pub struct HardThreshold(pub f64);
impl ThresholdFn for HardThreshold {
    fn apply(&self, x: f64) -> f64 {
        if x >= self.0 { x } else { 0.0 }
    }
    fn lipschitz(&self) -> f64 {
        f64::INFINITY   // HONEST
    }
}
```

A step function. Below the cutoff, activation is suppressed entirely;
at and above, activation passes through unchanged. This is the
cognitive-psychology formulation of spreading activation — ACT-R
uses something like this — and it was the first version shipped in
cowiki-rs.

It has an infinite Lipschitz constant. The crate declares this
honestly. The iteration does not converge under `HardThreshold` in
general; it can enter a limit cycle where some node oscillates between
"just below threshold" and "just above threshold" on alternating
iterations, which never stabilizes.

<div class="postmortem">

**Postmortem.** We shipped `HardThreshold` as the default for several
months. It felt right — the cognitive science literature uses it,
and for well-behaved corpora the iteration appeared to converge.

`PROOF.md` Finding #8 is the story of how we realized this was wrong.
While preparing a formal statement of convergence, we tried to write
down the contraction argument for `HardThreshold` and couldn't —
because there isn't one. Building a corpus that produced a visible
limit cycle took about 20 minutes with `proptest`. The
`SigmoidThreshold` path was already in the crate; we promoted it to
default the same day.

The `converged` flag in `SpreadResult` is the ward against this class
of bug recurring. If the iteration reaches the max-iteration limit
without the residual dropping below \\(\varepsilon\\), `converged` is
`false` and the caller knows not to trust the ranking. `HardThreshold`
sets off this flag reliably on any corpus with a non-trivial graph.

</div>

## The iteration

```rust
pub fn spread<T: ThresholdFn>(
    graph: &ScoredGraph,
    initial: &[f64],
    threshold: &T,
    config: &SpreadConfig,
) -> SpreadResult {
    let n = graph.len();
    let (row_ptr, col_idx, values) = graph.adj_transpose_csr();
    let d = config.decay;
    let eps = config.epsilon;
    let max_iter = config.max_iterations;

    let mut curr = initial.to_vec();
    let mut next = vec![0.0; n];
    let mut iterations = 0;
    let mut converged = false;

    for iter in 0..max_iter {
        iterations = iter + 1;

        for j in 0..n {
            let s = row_ptr[j];
            let e = row_ptr[j + 1];
            let mut acc = 0.0;
            for k in s..e {
                let i = col_idx[k];
                acc += values[k] as f64 * threshold.apply(curr[i]);
            }
            next[j] = d * acc + (1.0 - d) * initial[j];
        }

        let residual: f64 = curr.iter().zip(next.iter())
            .map(|(a, b)| (a - b).abs())
            .sum();

        std::mem::swap(&mut curr, &mut next);

        if residual < eps {
            converged = true;
            break;
        }
    }

    SpreadResult { activation: curr, iterations, converged }
}
```

The hot loop iterates the *transposed* CSR — for each target \\(j\\),
accumulate \\(\sum_i W_{ij} \cdot f(a^t_i)\\) — so the inner loop walks
incoming edges. This matches [the scored-graph
chapter's](scored-graph.md) argument for why the transposed CSR
exists.

One detail worth flagging: `curr` and `next` are swapped each iteration
rather than re-allocated, so there are no per-iteration allocations.
At \\(n = 500{,}000\\) with \\(f64\\) activations, each buffer is 4 MiB;
swapping rather than reallocating saves ~250 ms per query at scale.

## Convergence

<div class="claim">

**Claim.** The spread iteration is a contraction on
\\((\mathbb{R}^n, \lVert \cdot \rVert_1)\\) with Lipschitz constant
\\(d \cdot L\\). Provided \\(d \cdot L < 1\\), the iteration converges
to a unique fixed point at a geometric rate.

</div>

**Proof sketch.** Let \\(T : \mathbb{R}^n \to \mathbb{R}^n\\) be the
iteration operator,

\\[
T(a) = d \cdot W^\top f(a) + (1 - d) a^0.
\\]

We want to show \\(\lVert T(a) - T(b) \rVert_1 \le (d \cdot L) \lVert a - b \rVert_1\\)
for arbitrary \\(a, b \in \mathbb{R}^n\\).

Expand:

\\[
\lVert T(a) - T(b) \rVert_1
= d \cdot \lVert W^\top (f(a) - f(b)) \rVert_1
\\]

(the \\((1-d)a^0\\) term cancels because it's the same in both).

Because \\(W\\) is row-stochastic, \\(W^\top\\) is column-stochastic:
its columns sum to at most 1. For any vector \\(x\\),
\\(\lVert W^\top x \rVert_1 \le \lVert x \rVert_1\\). So:

\\[
\lVert W^\top (f(a) - f(b)) \rVert_1 \le \lVert f(a) - f(b) \rVert_1
\le L \cdot \lVert a - b \rVert_1
\\]

where the second inequality is \\(f\\)'s Lipschitz property applied
component-wise, then summed. Combining:

\\[
\lVert T(a) - T(b) \rVert_1 \le d \cdot L \cdot \lVert a - b \rVert_1.
\\]

If \\(d \cdot L < 1\\), this is a strict contraction, and Banach's
fixed-point theorem gives us the unique attractor plus the geometric
convergence rate. \\(\square\\)

The proof is implemented as a proptest in
`crates/spread/src/lib.rs::contraction_property`. It samples random
graphs and random pairs of activation vectors, runs one iteration,
and asserts the contraction inequality holds. It has run on ~10^6
random cases without a counterexample; any future refactor that breaks
it trips the test on commit.

## Geometric envelope

A consequence we use in tests: the residual \\(r_t = \lVert a^t - a^* \rVert_1\\)
satisfies \\(r_t \le (d \cdot L)^t \cdot r_0\\). So we can predict how
many iterations are needed to reach a given tolerance:

\\[
t \ge \frac{\log(\varepsilon / r_0)}{\log(d \cdot L)}
\\]

With \\(d = 0.8\\), \\(L = 0.5\\) (sigmoid default), \\(r_0 \le 1\\),
\\(\varepsilon = 10^{-14}\\), we need \\(\approx 35\\) iterations. The
default `max_iterations = 100` is comfortable headroom; in practice
the iteration converges in the mid-20s on most queries.

## Configuration

```rust
pub struct SpreadConfig {
    pub decay: f64,          // d, in (0, 1); default 0.8
    pub epsilon: f64,         // convergence tolerance; default 1e-14
    pub max_iterations: usize, // safety bound; default 100
}
```

The defaults are tuned for the knowledge-corpus regime we've
targeted. Dropping `epsilon` buys precision at the cost of
iterations (roughly \\(\log\varepsilon\\) extra). Raising `decay` towards
1 makes activation spread further (longer-tail reach) at the cost of
slower convergence; the default `0.8` is a compromise that converges
fast and still reaches 4–5 hops out on a typical corpus.

`max_iterations` is a safety net, not a target. If a query hits it,
`converged` is `false` and the caller should treat the result as
unreliable. In production, we've never observed this happen with
`SigmoidThreshold`.

## What the crate doesn't do

The crate does no query parsing, no ranking interpretation, no
ignition-vector construction. Those belong to the `wiki-backend::tfidf`
module. This crate does one thing: given a graph and an initial
vector, produce the fixed-point vector.

This discipline is what makes the whole system legible. The iteration
is `spread()`. The ignition is `tfidf::ignite()`. The selection is
`budget_knap::select()`. The composition is `cowiki::retrieve()` —
fifteen lines of code that call three primitives in sequence. If a
ranking comes out wrong, you can localize the fault to exactly one of
those four functions and nowhere else.
