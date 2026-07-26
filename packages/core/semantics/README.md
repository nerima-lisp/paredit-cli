# paredit-core-semantics

Lexical scope, binding tables, and cross-file project resolution.

## Responsibilities

Answering "what does this name refer to, here?" — the layer between a parsed
tree and any rule that needs to know whether two occurrences of a symbol are
the same thing:

- **Lexical scope.** The scope tree, which binder introduced a name, and which
  references a binding actually reaches once shadowing is accounted for.
- **Binding tables.** `BindingIndex` and the callable-scope view over local
  functions, macros and their bodies.
- **Definitions and references.** Where a definition is, and which occurrences
  refer back to it.
- **Project-wide resolution.** The cross-file table: package resolution, symbol
  resolution against it, constant value resolution, and the type view.

### What this package does not own

- **No reports and no rules.** It supplies the facts a rule reasons about; it
  never decides that something is wrong. Every `*_report` belongs to a feature
  package.
- **No edits.** Deciding that a rename is safe is a `core/edit` question, and
  performing it is a feature's.
- **No parsing.** Trees, spans and dialects come from `paredit-core-syntax`.
- **No filesystem access.** Project resolution is handed already-parsed trees;
  finding them is `paredit-core-workspace`'s job.
- **No ASDF system ordering.** `system_order` used to live here and was moved
  out: `:depends-on` ordering is project analysis, and keeping it here made
  core depend on two feature-level reports.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | The single largest inbound edge in the workspace: 140 references. Every scope and binding is expressed in terms of `SyntaxTree`, `ExpressionView`, `ByteSpan` and Common Lisp form knowledge. |
| `anyhow` | Fallible resolution paths, pending the `thiserror` conversion in §9.2. |
| `thiserror` | The typed errors that already exist here. |
| `proptest` (dev) | Properties over generated scope shapes. |

## Public API

| Module | Principal items |
| --- | --- |
| `semantics` | `NodeKey`, `Ty`, `evaluate_constant`, and the `typing`, `project`, `value` and `service` submodules |
| `lexical_scope` | `collect_unshadowed_symbol_references`, `value_capture` |
| `callable_scope` | `local_callable_names`, `common_lisp_local_callable_form`, `is_macro_callable_form` |
| `binding_index` | `BindingIndex` |
| `definition_reference` | Used only within the package today; kept because the builders already derive it |

Note that the root crate re-exports `lexical_scope` as `pub` and the other four
as `pub(crate)`, mirroring their original declarations. In particular
`domain::semantics` is **not** public API, despite the 1.1.0 CHANGELOG entry
describing it as one — it has always been `pub(crate) mod semantics`.

`semantics` carries a module-level `#![allow(dead_code, unused_imports)]` with
a written rationale for each retained item. That allowance is deliberate and
predates the split; read the note before deleting anything it covers.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a shadowing bug, or a reference attributed to the wrong binder | the scope tree and binding provenance are here |
| teaching scope analysis about a new binding form | the scope builders are here (the *form* itself is classified in `core/syntax`) |
| making cross-file symbol or package resolution agree with the reader | the project table is here |
| adding a fact that several unrelated reports would each otherwise re-derive | that is exactly what this package is for |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a report that consumes scope information | reports are feature packages; this package must not know they exist |
| deciding whether an edit is safe | that is `core/edit` |
| walking the filesystem to find files to resolve | that is `core/workspace` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
