# spread

The spreading-activation iteration. Takes a graph and an initial
activation vector; returns the fixed-point activation vector plus
diagnostics.

## Public API

```rust
use spread::{
    spread, SpreadConfig, SpreadResult,
    ThresholdFn, NoThreshold, HardThreshold, SigmoidThreshold,
};
```

### Types

```rust
pub trait ThresholdFn {
    fn apply(&self, x: f64) -> f64;
    fn lipschitz(&self) -> f64;
}

pub struct SpreadConfig {
    pub decay: f64,            // d, in (0, 1); default 0.8
    pub epsilon: f64,          // convergence tolerance; default 1e-14
    pub max_iterations: usize, // safety cap; default 100
}

pub struct SpreadResult {
    pub activation: Vec<f64>,
    pub iterations: usize,
    pub converged: bool,
}
```

### Threshold implementations

| type | `lipschitz()` | use when |
|---|---|---|
| `NoThreshold` | `1.0` | Pure linear propagation; PageRank-style. |
| `SigmoidThreshold { center, steepness }` | `steepness * 0.25` | **Default.** Smooth, convergent when `steepness ≤ 4`. |
| `HardThreshold(cutoff)` | `f64::INFINITY` | Research only — does not converge in general (see notes). |

`SigmoidThreshold::default()` → `{ center: 0.1, steepness: 2.0 }` (L = 0.5).

### Entry point

```rust
pub fn spread<T: ThresholdFn>(
    graph: &ScoredGraph,
    initial: &[f64],
    threshold: &T,
    config: &SpreadConfig,
) -> SpreadResult;
```

Requires: `initial.len() == graph.len()`. Panics if not.

### Utility

```rust
pub fn linear_contraction_distance(a: &[f64], b: &[f64]) -> f64;
```

L1 distance. Exposed for tests that verify the contraction bound
holds empirically.

## Invariants

<div class="claim">

**Contraction.** With \\(L = \texttt{threshold.lipschitz()}\\) and
\\(d = \texttt{config.decay}\\), the iteration operator is a
contraction on \\((\mathbb{R}^n, \lVert \cdot \rVert_1)\\) with
Lipschitz constant \\(d \cdot L\\). If \\(d \cdot L < 1\\), a
unique fixed point exists and the iteration converges at a
geometric rate.

</div>

<div class="claim">

**Envelope.** \\(r_t \le (d \cdot L)^t \cdot r_0\\) where
\\(r_t = \lVert a^t - a^* \rVert_1\\). For defaults \\(d=0.8,
L=0.5\\) and \\(\varepsilon = 10^{-14}\\), expected iterations
≈ 35.

</div>

<div class="claim">

**`converged` flag honesty.** Set to `true` only when the L1
residual between successive iterations drops below
`config.epsilon`. Reaching `max_iterations` without that
condition returns `converged = false`.

</div>

## The iteration

\\[
a^{t+1} = d \cdot W^\top f(a^t) + (1 - d) a^0
\\]

Implementation sketch (see `crates/spread/src/lib.rs` for the
full source):

```rust
let (row_ptr, col_idx, values) = graph.adj_transpose_csr();
let (mut curr, mut next) = (initial.to_vec(), vec![0.0; n]);

for iter in 0..config.max_iterations {
    for j in 0..n {
        let s = row_ptr[j];
        let e = row_ptr[j + 1];
        let mut acc = 0.0;
        for k in s..e {
            let i = col_idx[k];
            acc += values[k] as f64 * threshold.apply(curr[i]);
        }
        next[j] = config.decay * acc
                + (1.0 - config.decay) * initial[j];
    }
    let residual: f64 = curr.iter().zip(&next)
                            .map(|(a, b)| (a - b).abs()).sum();
    std::mem::swap(&mut curr, &mut next);
    if residual < config.epsilon { return Converged(curr, iter+1); }
}
```

Two properties of the implementation worth noting:

- **Transposed CSR, not forward.** Iterating incoming edges per
  target makes the inner loop \\(O(\text{in-deg}(j))\\);
  iterating the forward CSR would require an \\(O(n)\\) scan per
  target. See [scored-graph](scored-graph.md) for why both CSRs
  exist.

- **Buffer swap, not alloc.** `curr` and `next` are swapped each
  iteration rather than reallocated. At \\(n = 500{,}000\\) with f64
  activations, each buffer is 4 MiB; swapping saves ~250 ms per
  query.

## Examples

### Default configuration

```rust
let g = /* ScoredGraph */;
let initial = /* TF-IDF ignition vector, length g.len() */;

let config = SpreadConfig::default();
let result = spread(&g, &initial, &SigmoidThreshold::default(), &config);

assert!(result.converged);
println!("converged in {} iterations", result.iterations);
let top = result.activation.iter()
    .enumerate()
    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
    .unwrap();
println!("max activation: node {} with {}", top.0, top.1);
```

### Tighter convergence

```rust
let config = SpreadConfig {
    decay: 0.85,
    epsilon: 1e-18,
    max_iterations: 200,
};
```

### Pure PageRank-style (no non-linearity)

```rust
let result = spread(&g, &initial, &NoThreshold, &config);
```

## Notes

<div class="postmortem">

**`HardThreshold` was once the default.** It felt right — the
cognitive-psychology literature uses it, and on well-behaved
corpora the iteration *appeared* to converge. The trouble is that
with an infinite Lipschitz constant there is no contraction, and
a node can oscillate across the cutoff forever. `proptest` found
a limit cycle in ~20 minutes; commit that promoted
`SigmoidThreshold` to default: see `PROOF.md` Finding #8.

The `converged` flag is the durable guardrail: it will report
`false` on any threshold whose behavior breaks contraction, so
downstream code knows not to trust the ranking.

</div>

<div class="aside">

**When to use `NoThreshold`.** If your graph is already sparse and
well-behaved, `NoThreshold` (L = 1.0) is valid because
\\(d \cdot L = d < 1\\) provides the contraction by damping alone.
It's faster (no per-node function call) and produces marginally
different rankings than sigmoid. For most corpora the
difference doesn't matter; the default sigmoid stays because it's
what `runtime_audit` tests against.

</div>

## Proof obligations

- `contraction_property` proptest — samples random graphs and
  pairs of activation vectors, asserts
  \\(\lVert T(a) - T(b) \rVert_1 \le d \cdot L \cdot \lVert a - b \rVert_1\\).
- `envelope_property` — runs the iteration and checks that each
  iteration's residual is within the predicted geometric bound.
- `convergence_limit` — asserts the reported fixed point is
  within `epsilon` of the true limit (computed by oversampling).

All three have run ~\\(10^6\\) random cases each without a
counterexample. A future refactor that breaks any of them trips
the test on commit.
