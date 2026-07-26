# paredit-feature-inline

Inlining functions, bindings, lambdas, local functions, and symbol macros.

## Responsibilities

The inverse of extraction: replacing a name with the thing it names, and
substituting arguments correctly while doing so.

- **`refactor inline-function`** — replaces a call with the callee's body,
  substituting arguments for parameters. The bulk of the package, because
  substitution has to be right about evaluation order, argument reuse, and
  macro expansion.
- **`refactor inline-let`** — replaces a `let`-bound name with its initializer
  and drops the binding.
- **`refactor inline-lambda`** — replaces an immediately-applied lambda with
  its substituted body.
- **`refactor inline-local-function`** — the same for an `flet`/`labels`
  binding.
- **`refactor inline-symbol-macro`** — replaces a symbol macro with its
  expansion.

The refusals matter as much as the rewrites. Inlining is unsafe when an
argument would be dropped (its side effects lost) or duplicated (its side
effects repeated), so both require an explicit opt-in flag rather than being
decided silently.

### What this package does not own

- **No extraction.** `feature/extract` is the inverse and shares nothing but
  core.
- **No renaming.** Which is why one slice is *not* here — see below.
- **No scope analysis or span surgery.** Those are `core/semantics` and
  `core/edit`.

### Why `inline_literal_constant` is not in this package

It calls `collect_define_symbol_macro_reference_renames`, which belongs to
`domain::rename` — a feature package that does not exist yet. Rather than drag
the whole of `rename` (18,895 lines) forward out of §6's order, that slice
stays in the root crate and joins this package when `feature/rename` is
extracted. At that point this package gains a `paredit-feature-rename`
dependency, which is a legitimate feature-to-feature edge (§2.2 measured 89 of
them across the tree).

Worth noting how it was found: the reference is written `use super::rename::…`,
not `crate::domain::rename::…`, so a closure check that only looks for `crate::`
paths reports the feature as closed and the breakage surfaces later, at compile
time. `scripts/move-feature-package.py` now follows `super::` siblings too.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Bodies and call sites are subtrees; substitution is expressed over spans. |
| `paredit-core-semantics` | Deciding whether substituting a name captures or shadows something needs the binding tables. |
| `paredit-core-edit` | Span replacement, definition removal, and the shared reader-conditional refusals. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types, `safe_text!`. |
| `clap` | Argument parsing, confined to each slice's `cli`. |
| `serde_json` | JSON output. |
| `anyhow` | Fallible planning paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Properties over generated call sites. |

## Public API

Two names per slice, per §4.2 — the `clap` argument type and the function that
runs it. Five slices, so ten names, and `command.rs`/`dispatch.rs` see nothing
else.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1:

```text
src/
├── inline_function/
│   ├── domain.rs + domain/     substitution, call discovery, macro expansion
│   ├── usecase.rs + usecase/
│   └── cli.rs
├── inline_let/{domain.rs + domain/, usecase.rs + usecase/, cli.rs}
├── inline_lambda/{domain.rs, usecase.rs, cli.rs}
├── inline_local_function/{domain.rs, usecase.rs, cli.rs}
└── inline_symbol_macro/{domain.rs, usecase.rs, cli.rs}
```

Several slices have a `<layer>.rs` beside a `<layer>/` — Rust's 2018 style,
where the file is the module root and the directory holds its children. Both
halves must always move together.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a substitution that drops or duplicates a side effect | that is the core difficulty, and the refusals live in the slice's `domain` |
| fixing inlining through a macro expansion | `inline_function/domain/macro_expansion` |
| changing when an inline is refused, or which flag overrides it | the slice's `domain` states the rule; the flag is in its `cli` |
| adding a flag to any of the five commands | the slice's `cli` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| working on extraction | that is `feature/extract`, the inverse |
| renaming references as part of an inline | that is `feature/rename`'s job, which is why one slice waits for it |
| adding a refusal every refactoring needs | that is `core/edit`'s `mutation_safety` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
