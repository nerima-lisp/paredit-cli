# paredit-feature-form-transform

Reshaping call forms: threading, replacement, unwrapping, ordering, and splitting.

## Responsibilities

Transformations that change the *shape* of code without changing what it
computes, and without needing to know what any particular operator means:

- **`refactor thread-expression` / `unthread-expression`** — converts between
  nested calls and a threading pipeline, in both directions.
- **`refactor replace-forms`** — replaces every form matching a pattern with a
  replacement template.
- **`refactor unwrap-call`** — replaces a call with one of its arguments,
  dropping the wrapper.
- **`sort_definitions`** — reorders top-level definitions by name, keeping each
  definition's comments attached to it.
- **`split_file`** — distributes a file's definitions across several files.

The last two have **no `cli` layer**: they own no subcommand and are driven by
another command's workflow. A slice is not required to span all three layers,
and inventing an empty one to make the shape uniform would be worse than the
asymmetry.

### What this package does not own

- **No semantics.** These transformations are about form shape. Anything
  needing to know what a name refers to belongs elsewhere.
- **No extraction or inlining.** Those give and remove names;
  `feature/extract` and `feature/inline` own them.
- **No comment reflowing.** Keeping comments attached during a reorder uses
  `core/syntax`'s `leading_trivia`; this package does not reimplement it.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Forms are subtrees; `leading_trivia` keeps comments attached when definitions move. |
| `paredit-core-semantics` | The few checks that need to know a name is bound before reshaping around it. |
| `paredit-core-edit` | Span replacement and the shared reader-conditional refusals. |
| `paredit-core-workspace` | `split_file` writes several files, so it needs the workspace view. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| `clap` | Argument parsing, confined to each slice's `cli`. |
| `serde_json` | JSON output. |
| `anyhow` | Fallible planning paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Round-trip properties: threading then unthreading must reproduce the input. |

## Public API

Two names for each of the four slices that own a subcommand
(`thread_expression`, `unthread_expression`, `replace_forms`, `unwrap_call`),
per §4.2. `sort_definitions` and `split_file` publish their domain and use case
for their driving command to call.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1:

```text
src/
├── thread_expression/{domain.rs + domain/, usecase.rs, cli.rs}
├── unthread_expression/{domain.rs + domain/, usecase.rs, cli.rs}
├── replace_forms/{domain.rs + domain/, usecase.rs, cli.rs}
├── unwrap_call/{domain.rs, usecase.rs, cli.rs}
├── sort_definitions/{domain.rs + domain/, usecase.rs}     no cli
└── split_file/{domain.rs + domain/, usecase.rs}           no cli
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a threading conversion that loses an argument position | the round trip is this package's central property |
| fixing a reorder that detaches a comment from its definition | `sort_definitions` owns that, over `leading_trivia` |
| changing how a replacement template matches or substitutes | `replace_forms` |
| changing how a file is split | `split_file` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a transformation that needs to resolve names | it belongs with the feature that owns that analysis |
| changing how comments are parsed or re-emitted | that is `core/syntax` |
| adding a lint that suggests threading | rules live in `feature/lint-*` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
