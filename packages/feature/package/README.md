# paredit-feature-package

Common Lisp package and ASDF system analysis, plus the package refactoring.

## Responsibilities

Everything that reasons about *namespaces* rather than about code inside one:

- **`refactor rename-package`** — renames a package and every reference to it,
  across `defpackage`, `in-package`, package-qualified symbols and nicknames.
- **`package_report`** — what packages a file defines, uses, exports and
  imports. `PackageDefinitionReport` is the shared shape everything else here
  reads.
- **`dependency_report`** — the `:use` and ASDF `:depends-on` edges a file
  declares.
- **Unused-namespace reports** — `unused_package_report`,
  `unused_nickname_report`, `unused_export_report`.
- **Conflict and boundary reports** — `package_conflict_report`,
  `system_conflict_report`, `package_boundary_report`.

### What this package does not own

- **No project-wide graph analysis.** Cycles, impact and call graphs are
  `feature/project-analysis`.
- **No symbol renaming.** Renaming a *package* is here; renaming a function or
  variable is `feature/rename`.
- **No lint rules.** These are reports invoked as commands, not rules the lint
  engine runs.

### Why `dependency_report` is here, and why its `cli` is not

Section 5.2.1 assigns `dependency_report` to F2 project-analysis. Measurement
disagrees: `unused_package_report`, `unused_export_report` and
`package_boundary_report` all call `build_dependency_report` in production
code, so this package cannot be closed without it. It is also the better home
on the merits — it describes `defpackage :use` and ASDF `:depends-on` edges,
which is package knowledge, not graph analysis.

Its **`cli` layer** stays in the root crate for now, because that command calls
`definition_report::collect_definition_forms`, which belongs to
`feature/project-analysis`. It joins this package when F2 is extracted. A slice
whose layers straddle a boundary mid-migration is not a problem to design
around; the façade keeps it working either way.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | `defpackage`, `in-package` and package-qualified symbols are all syntax the reader normalises. |
| `paredit-core-semantics` | Resolving which package a symbol belongs to needs the project table. |
| `paredit-core-edit` | Span replacement and the shared mutation-safety refusals for the rename. |
| `paredit-core-workspace` | Package and system analysis is inherently multi-file. |
| `paredit-core-cli` | Input reading, atomic writes, shared argument types. |
| `clap` | Argument parsing, confined to each slice's `cli`. |
| `serde_json` | JSON report output. |
| `anyhow` | Fallible paths, pending §9.2. |
| `thiserror` | Typed failures. |
| `proptest` (dev) | Properties over generated package declarations. |

## Public API

One `(Args, run)` pair per slice that owns a subcommand, per §4.2, plus
`package_report`'s `PackageDefinitionReport` and `build_package_report`, which
`feature/remove-unused` consumes — a deliberate feature-to-feature edge, and
the reason this package had to be extracted before F9 despite §6 ordering them
the other way.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1, nine slices:

```text
src/
├── package/{domain.rs + domain/, usecase.rs, cli/}      the rename refactoring
├── package_report/          the shared package model
├── dependency_report/       domain + usecase only, cli waits for F2
├── unused_package_report/ … unused_nickname_report/ … unused_export_report/
├── package_conflict_report/ … system_conflict_report/
└── package_boundary_report/
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a package reference the rename missed | `package/domain/rename` owns occurrence discovery |
| changing what counts as a used package, nickname or export | the three unused_* slices |
| teaching the tool a new `defpackage` clause | `package_report` is the shared model everything here reads |
| changing how ASDF `:depends-on` is read | `dependency_report` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| analysing cycles or impact across a project | that is `feature/project-analysis` |
| renaming a function or variable | that is `feature/rename` |
| adding a lint rule about packages | rules are `feature/lint-*` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
