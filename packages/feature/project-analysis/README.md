# paredit-feature-project-analysis

Project-wide analysis: call graphs, cycles, impact, signatures and workspace reports.

## Responsibilities

The reports that answer questions about a codebase as a whole, rather than
about one form:

- **Call structure.** `call_report`, `call_graph_report`, `reachability_report`
  — who calls what, and what nothing reaches.
- **Cycles.** `call_cycle_report`, `class_cycle_report`, `struct_cycle_report`,
  `package_cycle_report`, `system_cycle_report` — dependency loops at five
  different granularities, over the shared Tarjan helper in `core/syntax`.
- **Change impact.** `impact_report` — what a proposed edit would reach, and at
  what risk level.
- **Shape and quality.** `signature_report`, `complexity_report`,
  `naming_report`, `form_report`.
- **Whole-workspace views.** `workspace_report`, `redefinition_report`,
  `undefined_package_report`, `unused_local_callable_report`.
- **`system_order`** — the ASDF `:depends-on` ordering resolver.

### What this package does not own

- **No lint rules.** None of these slices has a file under
  `domain/lint/rules`, which is exactly the test that separates a report
  invoked as a command from a rule the engine runs. Of the 157 `*_report`
  modules that were still in the root, 127 are rule-backed and belong to
  Phase 5; these eighteen are not.
- **No refactoring.** It reports; it never edits.
- **No package or definition model.** Those are `feature/package` and
  `feature/remove-unused`, which this package depends on.

### The workspace's only mutual cycle

`call_cycle_report` and `package_cycle_report` reference each other — the one
2-cycle §1.2 found across all 206 slices. §5.2.1's prescription is to
co-locate them, and both are here, so it resolves without any code change. It
is worth knowing that this was the entire cyclic risk in a 209k-line codebase.

### Why `system_order` rejoins here

It was moved out of `core/semantics` during Phase 2 because it depends on
`dependency_report` and `system_cycle_report`, which made core depend on
feature-level reports. ASDF ordering is project analysis rather than language
semantics, and this is the package it was waiting for.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | Every report walks trees, and the Tarjan SCC helper behind all five cycle reports lives there. |
| `paredit-core-semantics` | Call graphs and reachability are resolution questions across files. |
| `paredit-core-edit` | `impact_report` speaks in `RefactorRiskLevel`, which core/edit defines. |
| `paredit-core-workspace` | Every report here is multi-file by definition. |
| `paredit-core-cli` | Input reading, shared argument types, rendering helpers. |
| **`paredit-feature-package`** | `dependency_report`, which the cycle and system reports read. |
| **`paredit-feature-remove-unused`** | `definition_report`, the definition inventory several reports build on. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated call graphs. |

Both feature edges point at packages that had to be extracted first because a
member of *theirs* could not close without the slice in question — §5.2.1 put
`dependency_report` and `definition_report` in this package, and measurement
moved both out.

## Public API

One `(Args, run)` pair per slice that owns a subcommand, per §4.2.
`system_order` publishes no command; it is called by the system reports.

`#[non_exhaustive]` is deliberately absent (§9.4).

## Layout

Slice-first, per §3.1 — eighteen slices, each with the layers it has:

```text
src/
├── call_report/ … call_graph_report/ … reachability_report/
├── call_cycle_report/ … class_cycle_report/ … struct_cycle_report/
├── package_cycle_report/ … system_cycle_report/
├── impact_report/ … signature_report/ … complexity_report/
├── naming_report/ … form_report/ … workspace_report/
├── redefinition_report/ … undefined_package_report/
├── unused_local_callable_report/
└── system_order/          domain only, no command
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| fixing a call graph edge that is missing or spurious | `call_report` and `call_graph_report` |
| fixing a cycle report that misses a loop | all five share the Tarjan helper; the difference is what they treat as a node |
| changing how impact risk is computed | `impact_report` |
| adding a whole-project measurement | it is a new slice here |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a lint rule | if it has a `domain/lint/rules` file it is Phase 5's, not this package's |
| changing what a package exports | that is `feature/package` |
| removing anything a report finds | reports do not edit |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
