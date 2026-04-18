# Preface

This book documents `cowiki-rs` — an associative retrieval engine that searches
a corpus by spreading activation over a directed, typed, row-stochastic graph,
then packs the most valuable reachable documents under a token budget.

It also documents the first real corpus built on top of that engine: the
**SCOTUS Explorer**, a reader over ten thousand of the most-cited Supreme
Court opinions, live at <https://scotus.cowiki.tech>.

The engine and the corpus belong in the same book because neither is
interesting without the other. The engine is a set of proofs in working
code: a spreading-activation iteration whose convergence bound is
tight, a ≥½-OPT knapsack selection, a typed REM cycle that decays and
dreams. The corpus is where those proofs meet the messiness of real
data: half a million legal opinions exported from CourtListener, a
citation graph that was nearly empty until we enriched it with the
opinions' own prose, and a product UI that had to survive a content
surface that is 94% procedural and only 6% substantive.

## What this book is

A layered record of what we built and what we learned. Each primitive
crate in Part II carries a set of claims, a proof sketch, and the
measurement that validates it under real load. Each chapter of the
case study in Part IV describes a decision, the data that drove it, and
the consequence — including the decisions that turned out wrong the
first time.

There are three recurring callouts throughout the book. They are not
decorations.

<div class="claim">

**Claim.** A formal property the system must preserve. Usually an
invariant enforced by tests, often traceable back to `PROOF.md` or one
of the proptests in the primitive crates.

</div>

<div class="postmortem">

**Postmortem.** A concrete thing that went wrong on the way to the
system as it exists now. Kept in the book because the lesson is almost
always more durable than the fix.

</div>

<div class="aside">

**Aside.** Context that doesn't belong in the main flow but would
mislead by its absence. Historical notes, alternatives considered and
rejected, or gotchas in the surrounding ecosystem.

</div>

## What this book is not

It is not a tutorial. It assumes a reader comfortable with Rust, with
linear algebra up to sparse matrix arithmetic, and with the basic
vocabulary of information retrieval (TF-IDF, cosine similarity, BM25).
If those are new, read the [Rust book][rust-book] and chapter 3 of
Manning et al's *Introduction to Information Retrieval* first; they
cover most of the prerequisites in a single afternoon each.

It is not a survey. We have nothing useful to say about HNSW versus
IVF-PQ, about whether BERT embeddings beat SPLADE, about how LangChain
structures retrieval. These are legitimate questions that other people
are better placed to answer. What we have is one system built under a
specific thesis, with its proofs and its measurements, and we would
rather say that precisely than say everything loosely.

It is not finished. The book exists at the inflection point where the
first public corpus shipped and the next round of decisions has to be
made. Part IV's case study ends at the real deploy boundary —
`scotus.cowiki.tech` went live at DigitalOcean App Platform, costs $98
a month, serves queries in 3 ms — and the "what would we change"
section is written in present tense because we haven't done it yet.

## Who this is for

Three audiences, loosely:

- **Engineers** considering cowiki-rs as a retrieval layer for their own
  corpus. Parts I and III are aimed at you. The architecture is small
  enough that if you do read it, you'll know all of it within a day.

- **Writers and researchers** reasoning about retrieval systems, graph
  algorithms, or the legibility of ML infrastructure. Parts II and V
  are aimed at you. Every property is stated formally where we can
  state it formally, and measured where we can measure it.

- **Operators** running a cowiki-rs instance in production. Part IV's
  case study is aimed at you, especially the postmortems.

The three audiences overlap more than they don't.

## The voice

The book has three influences we try to keep distinct.

*Kleppmann* for the structural chapters — problem statement, the
choice and its trade-off, what the choice costs you at scale.

*Hipp* (the SQLite documentation) for the reference sections — one
fact per sentence, no sentence that isn't carrying weight.

*Cantrill* for the postmortems. The failures are the interesting
part; omitting them turns a book into a press release.

## Conventions

- **File paths** are written relative to the repository root:
  `crates/scored-graph/src/lib.rs`, not `/path/to/cowiki-rs/crates/...`.
- **Commit hashes** are shortened to 7 characters and linked to GitHub.
- **API surfaces** are quoted verbatim from the source as of the tag
  at the book's front matter; if you read the source at a newer tag,
  treat the book as documentation of intent, not ground truth.
- **Measurements** are given with their experimental setup. A number
  without a setup is a citation target, not a measurement.

## Source and contributions

The book source lives under `book/` in the same repository as the
engine:

```text
book/
├── book.toml
├── theme/overrides.css
└── src/
    ├── SUMMARY.md
    ├── preface.md
    ├── part1/  …
    ├── part2/  …
    └── …
```

Render locally with `mdbook serve book/`; deploy runs from
`.github/workflows/pages.yml` on every push to `main`. Corrections and
additions land through the same pull-request flow as code; there is no
separate docs-review path.

[rust-book]: https://doc.rust-lang.org/book/
