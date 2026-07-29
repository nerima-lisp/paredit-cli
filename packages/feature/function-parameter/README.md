# paredit-feature-function-parameter

Adding, removing, reordering and swapping function parameters, with call-site
updates — and reporting on the parameters a definition declares.

## Responsibilities

Two slices. The first is five subcommands and the hardest problem in the
refactoring set: changing a function's lambda list and every call to it,
consistently. The second, `unused_parameter_report`, only reads — but it reads
the *same* lambda list with the *same* validated parser, which is why it lives
here rather than in a reports package. Two parsers for one grammar would agree
until the day one of them changed.

- **`refactor add-function-parameter`** — inserts a parameter and supplies an
  argument at each call site.
- **`refactor remove-function-parameter`** — removes a parameter and drops the
  corresponding argument everywhere.
- **`refactor move-function-parameter`** / **`reorder-function-parameters`** /
  **`swap-function-parameters`** — permute the lambda list and permute every
  call's arguments to match.

The difficulty is call discovery, not the edit. A call can be shadowed by a
local binding of the same name, reached through a package qualifier, hidden
inside a macro expansion, or not be a call at all. Getting the lambda list right
and the call sites wrong produces code that still parses and no longer works —
which is why this feature carries by far the most tests.

### What this package does not own

- **No lambda-list linting.** `duplicate_parameter_report`,
  `unused_parameter_report`, `lambda_list_keyword_order_report` and
  `duplicate_lambda_list_keyword_report` are rules, and go to Phase 5's
  `feature/lint-*` packages. Section 5.2.1 groups them here; a rule that reports
  a bad lambda list and a refactoring that rewrites one are different products.
- **No scope analysis.** Whether a name at a call position is really this
  function is answered with `paredit-core-semantics`' binding tables.
- **No span surgery or safety refusals.** Those are `paredit-core-edit`.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Lambda lists and calls are subtrees; Common Lisp lambda-list keyword knowledge decides what is a parameter. |
| `paredit-core-semantics` | Call discovery is a scope question: a local binding shadowing the function name must not be rewritten. |
| `paredit-core-edit` | Span replacement and the shared mutation-safety refusals. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| `clap` | Argument parsing, confined to the slice's `cli`. |
| `serde_json` | JSON output. |
| `anyhow` | Fallible planning paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Properties over generated definitions and calls: the output must re-parse and preserve argument correspondence. |

## Public API

Five `(Args, run)` pairs, all from the one slice, per §4.2. Because the slice's
`cli` is a directory, its `mod.rs` hoists each pair from the submodule that
defines it, so the composition root sees `function_parameter::cli::…` and never
needs to know the internal file layout.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1 — one slice, three layers as names:

```text
src/function_parameter/
├── domain.rs + domain/     lambda-list model, call discovery, the five edits
├── usecase.rs
└── cli/                    args, render, and one module per subcommand
```

`domain.rs` sits beside `domain/`: Rust's 2018 style, where the file is the
module root and the directory holds its children. Both halves always move
together.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a call that should have been rewritten and was not, or vice versa | call discovery is the core difficulty, in `domain/calls` |
| fixing a shadowed local being mistaken for the function | same, though the scope facts come from core/semantics |
| teaching the lambda list about a new keyword | the lambda-list model is here (the keyword itself is classified in core/syntax) |
| adding a flag to any of the five subcommands | the slice's `cli` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule that reports a bad lambda list | rules are `feature/lint-*` |
| changing how a name resolves in general | that is `core/semantics` |
| changing how the file is written | that is `core/cli` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
