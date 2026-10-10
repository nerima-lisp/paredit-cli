# paredit-feature-form-transform

Reshaping call forms: threading, replacement, unwrapping, ordering, and splitting.

## Responsibilities

Transformations that change the *shape* of code. Structural validity does not
prove equivalent evaluation or lexical scope; callers must review those effects.

- **`refactor thread-expression` / `unthread-expression`** converts between
  nested calls and a threading pipeline, in both directions.
- **`refactor replace-forms`** replaces every form matching a pattern with a
  replacement template.
- **`refactor unwrap-call`** replaces a call with one of its arguments,
  dropping the wrapper.
- **`refactor unwrap-body`** removes a wrapper and its header arguments,
  retaining the body from the required zero-based `--body-start` argument
  index. Multiple body forms require a top-level or recognized sequence
  position. This is structural only, not scope- or meaning-preserving.
- **`sort_definitions`** reorders top-level definitions by name, keeping each
  definition's comments attached to it.
- **`split_file`** distributes a file's definitions across several files.

The last two have no `cli` layer here. The `sort-definitions` and `split-file`
workflows in `paredit-feature-remove-unused` call their use cases.

### What this package does not own

- **No general semantic-equivalence proof.** These transformations reshape
  forms and use bounded binding checks where needed. Broader name resolution
  belongs to `core/semantics` and the feature that owns the analysis.
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
| `anyhow` | Fallible planning paths. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Round-trip properties: threading then unthreading must reproduce the input. |

## Public API

Each slice that owns a subcommand exposes its arguments and entry point
(`thread_expression`, `unthread_expression`, `replace_forms`, `unwrap_call`,
`unwrap_body`). `sort_definitions` and `split_file` publish their domain and use case
for their driving command to call.

These workspace-only APIs do not use `#[non_exhaustive]`.

## Layout

Slice-first:

```text
src/
├── thread_expression/{domain.rs + domain/, usecase.rs, cli.rs}
├── unthread_expression/{domain.rs + domain/, usecase.rs, cli.rs}
├── replace_forms/{domain.rs + domain/, usecase.rs, cli.rs}
├── unwrap_call/{domain.rs, usecase.rs, cli.rs}
├── unwrap_body/{domain.rs + domain/, usecase.rs, cli.rs}
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
