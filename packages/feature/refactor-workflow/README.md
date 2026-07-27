# paredit-feature-refactor-workflow

The manifest-driven refactor workflow: plan, preview, check, diff and apply.

## Responsibilities

The batch face of the tool. Where every other feature performs one edit on
request, this one takes a *manifest* of edits and carries it through a
five-stage lifecycle with the safety guarantees that make it usable by an
agent:

- **plan** — turn a manifest into a set of concrete edits and summarise their
  risk.
- **preview** — show what would change without touching anything.
- **check** — verify the manifest still applies to the files as they are now.
- **diff** — render the change as a unified diff.
- **apply** — perform it, atomically across every file or not at all.

The interesting property is that `check` and `apply` are separated by content
hashes: a manifest planned against one state of the tree refuses to apply to a
different one. That is what makes the workflow safe to hand to something that
plans and applies in separate steps.

### What this package does not own

- **No individual edits.** Every operation a manifest can name belongs to
  another feature package. This one sequences them.
- **No risk model.** `RefactorRiskLevel` and the plan gate are
  `paredit-core-edit`'s, shared with anything else that reports risk.
- **No atomic write machinery.** Multi-file rollback is `paredit-core-cli`'s
  `io`, which exists precisely so this workflow is not the only thing that gets
  it right.

### Why this is a feature and not core

§8-2 leaves open whether this should be `core/refactor` instead, on the
grounds that it "composes the edit operations of several features" — and a
module that aggregates several features is composition root, not core (§11.5.1).

Measurement settles it: across all 76 files this package names exactly **one**
feature use case, `impact_report`. It reads risk from project analysis and
otherwise speaks only to core. One dependency is a feature depending on a
feature, not a composition root, so it stays a feature and §8-2 is closed.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Manifest edits are expressed over spans in parsed files. |
| `paredit-core-semantics` | Validating that a planned edit still refers to what it did. |
| `paredit-core-edit` | `RefactorOperation`, `RefactorPlanSummary`, the risk levels and the plan gate. |
| `paredit-core-workspace` | Manifests span many files, and `fs_identity` is what stops one file being processed twice. |
| `paredit-core-cli` | Multi-file atomic writes with rollback, expected-content preconditions, unified diff. |
| `blake3` | The content hashes that make check-then-apply safe across a gap in time. |
| **`paredit-feature-project-analysis`** | `impact_report`, for the risk a plan reports. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, manifest and report JSON, fallible paths. |
| `proptest` (dev) | Properties over generated manifests: apply is all-or-nothing. |

## Public API

One `(Args, run)` pair per subcommand the workflow owns, per §4.2.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1 — one slice with two layers:

```text
src/refactor/
├── usecase/     manifest model, planning, preview policy, validation
└── cli/         args, workflow, render, types, diff
```

No `domain` layer: the domain vocabulary this workflow needs already exists in
`core/edit`, and duplicating it here to make the shape symmetrical would be the
wrong kind of tidiness.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a stage or a flag to the refactor lifecycle | the five stages live here |
| changing what `check` verifies before `apply` proceeds | the precondition model is here |
| changing the manifest format | its schema and validation are here |
| fixing a partial apply | sequencing is here, though the atomic write itself is core/cli's |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding an operation a manifest can name | that operation belongs to its own feature |
| changing how risk is computed | that is `core/edit`, or `impact_report` |
| changing how files are written atomically | that is `core/cli` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
