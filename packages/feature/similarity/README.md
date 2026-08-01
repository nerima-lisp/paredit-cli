# paredit-feature-similarity

Similarity and duplicate reporting over workspace forms.

## Responsibilities

The commands that answer "is this code written twice?", and the scoring they
share:

- **`inspect similarity`** — ranks near-duplicate forms across a workspace by a
  similarity ratio, with thresholds, scope and overlap policies, and a CI gate.
  Each reported pair carries its clone type.
- **`inspect duplicates`** — reports forms that are structurally identical.
- **`inspect clone-classes` / `clone-sequences` / `clone-external` /
  `clone-threshold` / `clone-genealogy`** — the `clone_report` slice, which
  turns the pair list into something to act on: classes ranked by what
  extraction would save, sub-form runs no whole-form report can see, matches
  against a reference corpus, a threshold calibrated from the project, and the
  commit order that separates an original from its copies.
- **`form_similarity`** — the scoring primitive they are all built on,
  including the clone taxonomy (`classify_clone`). It lives here rather than in
  core because nothing else uses it; if another feature ever needs it, that is
  the moment to reconsider, not before.

### What this package does not own

- **No parsing, no scope analysis.** Trees and spans come from
  `paredit-core-syntax`.
- **No file discovery or writing.** Discovery is `paredit-core-workspace`; the
  I/O conventions are `paredit-core-cli`. This package never opens a file.
- **No other feature's reports.** It has no dependency on any
  `paredit-feature-*` crate and should acquire none — a similarity report that
  needs to know about renaming has been mis-scoped.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Forms are compared as parsed subtrees, so scoring needs `ExpressionView`, spans and dialect. |
| `paredit-core-workspace` | `--include`/`--exclude` resolve to a workspace scan behind the use case's source port. |
| `paredit-core-cli` | Shared argument types (`DialectArg`, `OutputFormat`), input reading, and the `safe_text!` rendering guard. |
| `clap` | Argument parsing — confined to each slice's `cli/` directory, which a contract test enforces. |
| `serde_json` | JSON report output. |
| `anyhow` | Fallible workflow paths, pending §9.2. |
| `thiserror` | `SimilarityReportOptionsError`, which §9.2 names as the pattern to generalise. |
| `proptest` (dev) | Properties over generated form pairs. |

## Public API

The composition root needs exactly two names per slice — the `clap` argument
type and the function that runs it (§4.2). Everything else is internal:

```rust,ignore
pub use similarity_report::cli::{SimilarityReportArgs, similarity_report};
pub use duplicate_report::cli::{DuplicateReportArgs, duplicate_report};
pub use clone_report::cli::{
    CloneClassReportArgs, CloneExternalReportArgs, CloneGenealogyReportArgs,
    CloneSequenceReportArgs, CloneThresholdReportArgs, clone_classes, clone_external,
    clone_genealogy, clone_sequences, clone_threshold,
};
```

`command.rs` and `dispatch.rs` in the root reference those names and
nothing more. Keeping that surface at two names per slice is what makes the
root's command tree mechanical rather than a second place where a feature's
internals leak.

The root also re-exports `domain::similarity_report` and
`application::usecase::similarity_report` through the façade, because
`benches/similarity_report.rs` uses the public library API and must keep
building unchanged.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1 — the layers are names, not directories:

```text
src/
├── form_similarity.rs          shared scoring primitive and clone taxonomy
├── similarity_report/
│   ├── domain/                 scoring, report model, options, classification
│   ├── usecase/                orchestration behind a source port
│   └── cli/                    args, cache, workflow, render, types
├── clone_report/
│   ├── domain/                 classes, sequences, external, calibration, genealogy
│   └── cli/                    args, collection, git port, workflow, render
└── duplicate_report/
    ├── domain.rs
    ├── usecase.rs
    └── cli/
```

`clone_report` depends on `similarity_report`, never the reverse. Pair
classification therefore lives in `similarity_report::domain::classify`, beside
the form model that both need, rather than in the slice built on top of it.

**Do not add `domain/`, `application/` or `presentation/` directories at the
top level of this package.** That would reproduce inside the package the exact
problem the split exists to fix: one feature's change spread across three
trees. A slice grows a subdirectory per layer only when that layer has more
than one file, as `similarity_report` does and `duplicate_report` does not.

## Complexity

Everything in this package is built on comparing forms to other forms, so its
cost is quadratic in the candidate count before any pruning. The mitigations
below are load-bearing, not optimisations: removing one does not make a report
slower, it makes it stop returning on inputs that are perfectly ordinary.

| Stage | Cost | What holds it down |
| --- | --- | --- |
| Pair enumeration | `O(n²)` in candidate forms | Size-based pruning rejects pairs whose node or leaf counts cannot reach the threshold, before any tree-edit distance is computed. `--max-candidates` and `--max-comparisons` are hard budgets on top. |
| Overlap suppression (`--overlap-policy maximal`) | `O(m log m)` in *matched* pairs | A span forest per file pair, not a pairwise containment test. |
| Sub-form runs (`inspect clone-sequences`) | `O(w)` run lengths per list of width `w` | `MAX_SUPPORTED_RUN_LENGTH`. |

### `--overlap-policy maximal` is the default, and the degenerate case

`maximal` suppresses a match that is wholly contained by a higher-ranked one,
which is what makes the report readable: without it, a duplicated function also
reports its body, and its body's body. The trap is that the suppression runs
over the *matched* pair set, and on a repetitive or generated corpus — one
where nearly every form matches nearly every other — that set is itself
quadratic in the candidate count. A pairwise containment check over it is
therefore quartic in the input, which does not look slow on a test fixture and
does not return at all on a real vendored directory.

So containment is not decided pair by pair. `suppress_contained_pairs` in
`similarity_report/domain/reports.rs` groups pairs by their (ordered) file pair,
builds a `SpanForest` over each side — form spans within one tree are nested or
disjoint, so containment is a forest rather than an arbitrary partial order —
and walks it with Euler ranges against a Fenwick tree. **Do not replace this
with the obvious nested loop.** It is the obvious nested loop that this exists
to avoid, and the input that exposes the difference is a corpus, not a unit
test.

### `MAX_SUPPORTED_RUN_LENGTH`

`clone_report/domain/sequence.rs` caps a reported run of adjacent sibling forms
at 64. The cap is cheap to justify twice over: a run longer than 64 is almost
certainly a whole body, which the form-shaped reports already cover, and
enumerating every run length up to a file's widest list is exactly what makes
naive run detection quadratic in that width. Raising it trades a class of
finding nobody asked for against a cost that grows with the worst file in the
tree.

### Not paying the quadratic cost twice

The mitigations above make one run affordable. `similarity_report/cli/cache.rs`
is about the *second* run: with `--cache-dir`, `inspect similarity` stores the
finished report under a content-addressed key covering the tool version, every
analysis option, and each selected file's path, dialect and content hash. A hit
means the identical question was already answered, so there is no invalidation
logic — hashing the corpus is a linear pass that buys back a quadratic one.

`--output` and `--fail-on-duplicates` are deliberately outside the key; the gate
is recomputed from the current run's flag. Anything that changes what a report
*says* must therefore reach the key, or a stale answer becomes reachable. See
`docs/src/reference/workspace-inputs.md` for the user-facing description.

### Measuring a change

`benches/similarity_report.rs` is where performance-sensitive changes to this
package are measured. Its scenarios exist specifically to pin the pruning
behaviour that is quadratic in the candidate count — `repeated-shape` is the
degenerate corpus, `node-count-pruned` and `leaf-count-pruned` are the two size
filters — so a change that defeats a filter shows up as a shape change across
input sizes rather than a flat slowdown.

Run it against a baseline revision rather than reading absolute numbers:

```console
$ ./scripts/bench-compare.sh
```

See `docs/src/reference/benchmarks.md` for what that reports and why the
confidence interval, not the point estimate, is the number to act on.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| changing how similarity is scored, or what counts as a duplicate | `form_similarity` is the one scorer |
| changing what makes a pair Type-1, Type-2 or Type-3 | `form_similarity::classify_clone` is the one classifier, and five reports read it |
| changing how clone classes are grouped or ranked | `clone_report/domain/class.rs` |
| adding a threshold, scope or overlap policy | options live in the slice's `domain` |
| changing the report's JSON or text rendering | the slice's `cli/render.rs` |
| adding a flag to either command | the slice's `cli/args.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a lint rule about duplication | rules live in `feature/lint-*`; a report and a rule are different products |
| making the scorer available to another feature | move `form_similarity` down into core at that point, rather than depending on this package |
| adding a new subcommand unrelated to similarity | it is its own feature package |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
