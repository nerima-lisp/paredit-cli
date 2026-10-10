# paredit-feature-refactor-workflow

Manifest-driven refactoring and snapshot-relative bulk editing.

## Responsibilities

This package carries a *manifest* of edits through a reviewable lifecycle:

- **plan**: generate a manifest for a selected symbol in the input files and
  summarise the proposed edits' risk.
- **preview**: show what would change without touching anything.
- **check**: verify the manifest still applies to the files as they are now.
- **diff**: render the change as a unified diff.
- **apply**: stage writes with expected-content checks and rollback on failure.
  Rollback can itself fail and is reported; this is not a crash-atomic
  multi-file transaction. Impact-area groups are independent batches.

The interesting property is that `check` and `apply` are separated by content
hashes: a manifest planned against one state of the tree refuses to apply to a
different one. Hash checks detect stale plans; they do not establish semantic
equivalence or prevent every possible concurrent filesystem mutation.

`refactor bulk` takes a strict version-1 request for one file. It supports
`wrap` and syntactic `rename-in-form` only. Paths resolve against the original
tree, actual replacement spans must not conflict, and the final output must
parse before it can be written. Bytes outside replacements are preserved.
The default JSON contains the proposed source; `--diff` is read-only and
`--write` uses an expected-content precondition. Rename does not resolve
bindings or prove that behavior is preserved.

### What this package does not own

- **No general edit implementation.** Manifest operations are composed here;
  bulk lowering reuses syntax wrapping and the rename feature's occurrence
  selection.
- **No risk model.** `RefactorRiskLevel` and the plan gate are
  `paredit-core-edit`'s, shared with anything else that reports risk.
- **No atomic write machinery.** Multi-file rollback is `paredit-core-cli`'s
  `io`, which exists precisely so this workflow is not the only thing that gets
  it right. Callers must still describe their own batching and failure scope.

### Why this is a feature and not core

It implements user-facing refactor workflows rather than a general syntax or
I/O primitive. Its feature dependencies provide impact analysis, change
summaries, and syntactic rename selection.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Manifest edits are expressed over spans in parsed files. |
| `paredit-core-semantics` | Validating that a planned edit still refers to what it did. |
| `paredit-core-edit` | `RefactorOperation`, `RefactorPlanSummary`, the risk levels and the plan gate. |
| `paredit-core-workspace` | Manifests span many files, and `fs_identity` is what stops one file being processed twice. |
| `paredit-core-cli` | Staged writes with rollback, expected-content preconditions, unified diff. |
| `paredit-core-safety` | Undo journals and post-write verification. |
| `blake3` | Manifest content hashes for stale-plan detection. |
| **`paredit-feature-project-analysis`** | `impact_report`, for the risk a plan reports. |
| `paredit-feature-rename` | Syntactic occurrence selection for bulk rename. |
| `paredit-feature-change-summary` | Compact apply summaries. |
| `libc`, `cap-std` | Filesystem operations for workflow artifacts. |
| `clap`, `serde`, `serde_json`, `thiserror` | Arguments, strict bulk requests, manifest and report JSON, fallible paths. |
| `proptest` (dev) | Generated manifest and refusal cases. |

## Public API

One argument type and entry point per subcommand the workflow owns.

`refactor_bulk` exports `RefactorBulkArgs` and `refactor_bulk`.

`refactor::cli::manifest::status` exports the decision vocabulary:
`RefactorManifestChecks` with `ManifestPolicy`, `ManifestOutputs` and
`RefactorManifestMismatchCounts`, plus `RefactorApplyOutcome`.

### Why the manifest decision takes one struct

`refactor_manifest_decision` used to take `(bool, bool, usize, usize, usize,
usize)`, two adjacent booleans followed by four adjacent counts, none of
which the compiler could tell apart. It decides whether a refactor may be
applied, so a caller that swapped `stale_file_count` and `parse_error_count`
produced a different verdict and a different exit code with nothing to catch
it. This is the same "request in, decision out" shape the architecture guide
states for application use cases.

The three per-file result types (`check`, `diff`, `apply`) also stopped
storing `input_hash_matches`, `output_hash_matches` and
`manifest_flags_match`. All three are derivable from fields the same struct
already holds, so storing them made three second sources of truth that could
disagree with the hashes beside them, the problem `stale()` was written to
avoid, applied to only one of four cases.

## Layout

The manifest and bulk workflows are separate slices:

```text
src/
├── refactor/
│   ├── usecase/     manifest model, planning, preview policy, validation
│   └── cli/         args, workflow, render, types, diff
└── refactor_bulk/
    ├── domain.rs   typed request, original-tree lowering, conflict validation
    └── cli.rs      arguments, read/preview/write, report
```

The manifest workflow reuses the domain vocabulary in `core/edit`; bulk owns
its narrower typed request.

## When you change this package

| You are changing... | It belongs here because... |
| --- | --- |
| adding a stage or a flag to the refactor lifecycle | the five stages live here |
| changing what `check` verifies before `apply` proceeds | the precondition model is here |
| changing the manifest format | its schema and validation are here |
| fixing a partial apply | sequencing is here, while staged writes and rollback live in core/cli |

| You are changing... | It does **not** belong here because... |
| --- | --- |
| adding an operation a manifest can name | that operation belongs to its own feature |
| changing how risk is computed | that is `core/edit`, or `impact_report` |
| changing how files are written atomically | that is `core/cli` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
