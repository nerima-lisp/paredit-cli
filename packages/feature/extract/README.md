# paredit-feature-extract

Extracting a selection into a function, a local function, or a constant.

## Responsibilities

Three commands that all answer "give this expression a name", differing only in
where the name goes:

- **`refactor extract-function`** — lifts a selected form into a new top-level
  function and replaces it with a call. Infers the parameter list from the
  free variables the selection captures, which is the bulk of the package.
- **`refactor extract-local-function`** — the same, into an enclosing
  `flet`/`labels` binding rather than the top level.
- **`refactor extract-constant`** — lifts a literal into a named constant.

Parameter inference is where the difficulty lives: deciding which names a
selection genuinely captures, which are shadowed, which are bound by an
enclosing form, and which are operators rather than values.

### What this package does not own

- **No inlining.** `feature/inline` is the inverse operation and a separate
  package; the two share nothing but core.
- **No scope analysis.** Which names a form captures is answered by
  `paredit-core-semantics`; this package decides what to do with the answer.
- **No span surgery.** `replace_span` and top-level insertion are
  `paredit-core-edit`'s, so no feature open-codes byte edits.
- **No safety rules of its own.** Reader-conditional refusals come from
  `core/edit`'s `mutation_safety`, stated once for every refactoring.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Selections are subtrees; Common Lisp operator and binding-form knowledge drives parameter inference. |
| `paredit-core-semantics` | Scope and binding tables decide which names a selection captures. |
| `paredit-core-edit` | `replace_span`, top-level insertion, and the shared mutation-safety refusals. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types, `safe_text!`. |
| `clap` | Argument parsing, confined to each slice's `cli`. |
| `serde_json` | JSON output. |
| `anyhow` | Fallible planning and workflow paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Properties over generated selections. |

## Public API

Two names per slice, per §4.2 — the `clap` argument type and the function that
runs it. `command.rs` and `dispatch.rs` in the root see nothing else:

- `extract_function::cli::{ExtractFunctionArgs, extract_function}`
- `extract_local_function::cli::{ExtractLocalFunctionArgs, extract_local_function}`
- `extract_constant::cli::{ExtractConstantArgs, extract_constant}`

`extract_local_function` uses `extract_function::domain`'s parameter inference
directly. That is a within-package dependency between slices, which is exactly
what a package boundary is for: the two would otherwise have to duplicate
inference or push it into core prematurely.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1:

```text
src/
├── extract_function/
│   ├── domain.rs + domain/     inference, rewriting, syntax helpers
│   ├── usecase.rs
│   └── cli.rs
├── extract_local_function/{domain.rs, usecase.rs, cli.rs}
└── extract_constant/{domain.rs, usecase.rs, cli.rs}
```

`extract_function/domain.rs` sits **beside** `extract_function/domain/`: that
is Rust's 2018 module style, where the file is the module root and the
directory holds its children. Both must move together — moving only the
directory leaves the module root behind and the package will not resolve.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing an extraction that captures the wrong parameters | inference lives in `extract_function/domain` |
| fixing a shadowed name being treated as free | same, though the scope facts come from core/semantics |
| changing where the extracted definition is inserted | the slice's `domain` decides placement |
| adding a flag to any of the three commands | the slice's `cli` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| working on inlining | that is `feature/inline`, the inverse operation |
| adding a safety refusal every refactoring needs | that is `core/edit`'s `mutation_safety` |
| changing how a file is written | that is `core/cli` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
