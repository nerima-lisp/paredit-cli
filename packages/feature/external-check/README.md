# paredit-feature-external-check

Cross-checking a refactor against an external Lisp implementation's own
diagnostics.

## Responsibilities

Every other analysis in this workspace reasons about the text. That reasoning
is good and it is not the same thing as the code still compiling. This package
is the one place that asks a real implementation:

- **Running `compile-file`.** SBCL's compiler is the closest thing Common Lisp
  has to a type checker, and its warnings cover exactly the class of mistake a
  syntactic refactor can introduce: an undefined variable a rename missed, an
  arity a signature change broke, a `defmethod` with no matching generic.
- **Comparing two runs.** A baseline distinguishes diagnostics introduced by a
  refactor from diagnostics that were already present.
- **Locating a diagnostic in the tree.** The implementation reports `in: DEFUN
  BAR`; this package maps that back to the definition's span, so a finding is
  navigable like every other finding in this tool.

### What this package does not own

- **No process handling.** `paredit-core-safety::external` spawns, waits within
  a budget, and parses the transcript. This package decides what to run it on
  and what the answer means.
- **No parsing of Lisp.** The tree comes from `paredit-core-syntax`.
- **No writes.** It is an `inspect` command and reads only.

## Compiling is executing

`compile-file` runs the file's macros, its `eval-when (:compile-toplevel)`
forms, and its `#.` read-time evaluation. Pointing this command at code you
would not run is the same act as running it. That is why `--implementation`
has no default: choosing to invoke an implementation is a decision the caller
makes explicitly, not one they fall into.

The compilation output goes to a temporary directory, so the source tree is
unchanged by a check that is supposed to be read-only.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-safety` | The process runner and the SBCL transcript parser. |
| `paredit-core-syntax` | Mapping a diagnostic's `in:` context back to a definition's span. |
| `paredit-core-cli` | The shared report envelope, argument types, and gate. |
| `clap` | Its own `cli` module, as every feature has. |
| `serde_json` | Baselines are JSON, and so is the report. |
| `anyhow` | The workflow's fallible boundary. |

## Public API

| Module | Principal items |
| --- | --- |
| `external_diagnostics_report` | `ExternalDiagnosticsReportArgs`, `external_diagnostics_report` |

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| teaching the tool a second implementation (CCL, ECL, ABCL) | the implementation table is here |
| changing what counts as an *introduced* diagnostic | baseline comparison is here |

| You are… | and it does **not** belong here because… |
| --- | --- |
| changing how a process is spawned or timed out | that is `paredit-core-safety::external` |
| adding an analysis this tool can do itself | do it in Rust; this package exists for what it cannot |
