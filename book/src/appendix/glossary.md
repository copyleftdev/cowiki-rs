# Glossary

**Activation vector** — A length-\\(n\\) real-valued vector giving
each node's current activation under the spreading-activation
iteration. Nodes with larger values are "more relevant" to the
query that produced the initial vector.

**Authored edge** — An edge whose existence was intentionally
declared by a human (wiki author, court's opinion, paper's
bibliography) rather than inferred from similarity or statistical
co-occurrence. The distinction is discussed in [Part I Chapter
3](../part1/the-typed-graph-bet.md).

**Budget knapsack** — The 0/1 knapsack problem used to select a
subset of retrieved items under a total-cost cap. The cost is
token count. See [Part II Chapter 3](../part2/budget-knap.md).

**Cert denial** — A certiorari denial. In SCOTUS terminology, a
one-paragraph order declining to hear a case. They accumulate
citation counts from lower courts but have near-empty opinion
bodies, which broke the initial top-N ranking for SCOTUS
Explorer. See [Part IV Chapter 5](../part4/landmarks.md).

**Contraction** — A mapping \\(T : X \to X\\) whose Lipschitz
constant is strictly less than 1. The spread iteration is a
contraction when \\(d \cdot L < 1\\), which guarantees a unique
fixed point.

**Corpus** — A set of documents with, in cowiki-rs's terms, an
authored link graph. Plural: *corpora*.

**CSR** — Compressed Sparse Row. The storage format used for the
graph's adjacency matrix. Three arrays (`row_ptr`, `col_idx`,
`values`) that together represent a sparse matrix without storing
its zero entries.

**Decay** — The REM-cycle operation that multiplicatively reduces
all edge weights by a small factor, so unused edges fade over
time.

**Dream** — The REM-cycle operation that proposes new edges
between pairs of nodes that have co-activated frequently in
recent queries despite not being directly connected in the
authored graph.

**Ignition** — The step that converts a query string into the
initial activation vector \\(a^0\\) used to seed the spread
iteration. In cowiki-rs the ignition is TF-IDF-based.

**Lipschitz constant** — The smallest \\(L\\) such that
\\(|f(x) - f(y)| \le L |x - y|\\) for all \\(x, y\\). A function's
Lipschitz constant bounds how much the output can change per
unit change of input.

**mdBook** — The static-site generator this book is built with.
Rust tool; produces a navigable HTML site from Markdown sources.

**Prune** — The REM-cycle operation that marks nodes with low
recent activation and no recent touches as *dead*, removing them
from the spread and knapsack without compacting the graph's
storage.

**REM cycle** — The composed decay + prune + dream operation that
cowiki-rs runs on `/api/maintain`. Named after the analogous
consolidation in human sleep.

**Row-stochastic** — A non-negative matrix whose rows each sum
to 1. A graph's adjacency matrix is row-stochastic when each
source node's outgoing weights are normalized to a probability
distribution over its targets.

**SCOTUS** — Supreme Court of the United States.

**Spreading activation** — The retrieval algorithm cowiki-rs is
built around. Origins in cognitive psychology (Collins & Loftus
1975); reinvented multiple times in information retrieval since.
See [Part I Chapter 1](../part1/why-spreading-activation.md).

**TF-IDF** — Term Frequency–Inverse Document Frequency. The term-
weighting scheme used by `wiki-backend::tfidf` to construct the
initial activation vector from a query string. Not glamorous,
effective at the ignition job.

**`.cowiki/`** — The persistence directory cowiki-rs writes
alongside a corpus. Contains SQLite for structured metadata plus
three sidecar files for the mmap-ready CSR graph arrays.

<!-- TODO(next slice): expand as needed while writing the
     remaining chapters; keep entries alphabetical. -->
