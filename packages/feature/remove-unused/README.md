# paredit-feature-remove-unused

Removing unused bindings, control forms and definitions, and moving definitions between files.

## Responsibilities

Deleting code that nothing reaches, and relocating code that belongs elsewhere
— two problems that share the same prerequisite, knowing what actually
references what:

- **`refactor remove-unused-binding`** — drops a binding no body reads.
- **`refactor remove-unused-control`** — drops a control form whose result is
  discarded and whose body has no effect.
- **`remove_unused_definition` / `definition_removal`** — drops a top-level
  definition nothing calls, honouring an export policy so a public API is not
  silently deleted.
- **`remove_definition`** — removes one named definition on request.
- **`definition_report`** — the inventory of what a file defines and which of
  those are unused, which the removal slices read.
- **`definition_movement`** — moves a definition to another file, and drives
  `sort_definitions` and `split_file`.

"Unused" is meaningful only relative to an export boundary: a function unused
inside its file may be the package's entire purpose, so removal consults
`package_report` for what is exported before deciding anything.

### What this package does not own

- **No reachability analysis of its own.** Which names a form references comes
  from `paredit-core-semantics`.
- **No package model.** What a package exports is `feature/package`'s
  `package_report`, which this package reads — see below.
- **No form reshaping.** `sort_definitions` and `split_file` are
  `feature/form-transform`'s; `definition_movement` drives them.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Definitions and binding forms are subtrees. |
| `paredit-core-semantics` | "Unused" is a reachability question over the binding tables. |
| `paredit-core-edit` | Span removal and the shared mutation-safety refusals. |
| `paredit-core-workspace` | Unused-definition analysis and definition movement are multi-file. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| **`paredit-feature-package`** | `PackageDefinitionReport` and `build_package_report`: a definition that is exported is not unused. This is the edge that forced F4 to be extracted before this package, against §6's stated order. |
| **`paredit-feature-form-transform`** | `definition_movement` drives `sort_definitions` and `split_file`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, output, fallible paths. |
| `proptest` (dev) | Properties over generated definition sets. |

Two feature-to-feature dependencies, both deliberate — §2.2 measured 89 such
edges across the tree, and refusing them would mean duplicating the package
model or the file-splitting logic.

## Public API

Five `(Args, run)` pairs, per §4.2. One is aliased: `definition_removal`
publishes a run function called `remove_definition`, which is also the name of
a slice in this package, so the re-export is `remove_definition as
run_remove_definition`.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1, seven slices:

```text
src/
├── remove_unused_binding/{domain.rs + domain/, usecase.rs, cli.rs}
├── remove_unused_control/{domain.rs, usecase.rs, cli.rs}
├── remove_unused_definition/{domain.rs + domain/, usecase.rs}
├── remove_definition/{usecase.rs}          usecase only
├── definition_report/{domain.rs, usecase.rs + usecase/, cli/}
├── definition_removal/{cli/}               cli only
└── definition_movement/{cli/}              cli only
```

Three slices have a single layer. `remove_definition` is a use case with no
domain rules of its own; `definition_removal` and `definition_movement` are
workflows over the other slices. Section 3.1's rule is one directory per slice,
not one of every layer per slice.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing something being deleted that is still used | reachability is consulted here, even though it is computed in core/semantics |
| changing the export policy that protects a public definition | `remove_unused_definition/domain/policy.rs` |
| fixing a removal that leaves stray whitespace or a dangling comment | the removal rewrites are here |
| changing how a definition moves between files | `definition_movement` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| changing what "exported" means | that is `feature/package` |
| changing how definitions sort or how a file splits | that is `feature/form-transform` |
| changing how references are resolved at all | that is `core/semantics` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
