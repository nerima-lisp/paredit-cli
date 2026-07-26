# paredit-feature-similarity

Similarity and duplicate reporting over workspace forms.

## Responsibilities

Two commands that answer "is this code written twice?", and the scoring they
share:

- **`inspect similarity`** — ranks near-duplicate forms across a workspace by a
  similarity ratio, with thresholds, scope and overlap policies, and a CI gate.
- **`inspect duplicates`** — reports forms that are structurally identical.
- **`form_similarity`** — the scoring primitive both are built on. It lives
  here rather than in core because nothing else uses it; if a third feature
  ever needs it, that is the moment to reconsider, not before.

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

```rust
pub use similarity_report::cli::{SimilarityReportArgs, similarity_report};
pub use duplicate_report::cli::{DuplicateReportArgs, duplicate_report};
```

`command.rs` and `dispatch.rs` in the root reference those four names and
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

```
src/
├── form_similarity.rs          shared scoring primitive
├── similarity_report/
│   ├── domain/                 scoring, report model, options
│   ├── usecase/                orchestration behind a source port
│   └── cli/                    args, workflow, render, types
└── duplicate_report/
    ├── domain.rs
    ├── usecase.rs
    └── cli/
```

**Do not add `domain/`, `application/` or `presentation/` directories at the
top level of this package.** That would reproduce inside the package the exact
problem the split exists to fix: one feature's change spread across three
trees. A slice grows a subdirectory per layer only when that layer has more
than one file, as `similarity_report` does and `duplicate_report` does not.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| changing how similarity is scored, or what counts as a duplicate | `form_similarity` is the one scorer |
| adding a threshold, scope or overlap policy | options live in the slice's `domain` |
| changing the report's JSON or text rendering | the slice's `cli/render.rs` |
| adding a flag to either command | the slice's `cli/args.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a lint rule about duplication | rules live in `feature/lint-*`; a report and a rule are different products |
| making the scorer available to another feature | move `form_similarity` down into core at that point, rather than depending on this package |
| adding a new subcommand unrelated to similarity | it is its own feature package |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
