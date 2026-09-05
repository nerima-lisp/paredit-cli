# paredit-core-lint-engine

The lint rule contract and the single pass that runs a rule set.

## Responsibilities

The mechanism a lint suite runs on, with no opinion about which lints exist:

- **The rule contract.** `LintRule` — a rule declares *which* nodes it wants
  (`head_filter`) and *what* to say about one (`check`). It never walks the
  document itself. That inversion is what lets 130+ rules share one pass.
- **The single pass.** One pre-order walk serves every rule: head-specific
  rules are reached through an operator index, shape rules see every node, and
  whole-document rules get the tree once before the walk.
- **The vocabulary.** Findings, fixes, fixability, severity, categories, and
  the ordering types that put findings back into report order.
- **Selection and gating.** Turning `--rule`/`--exclude`/`--category` into an
  active rule set, summarising a run, and judging it against a CI gate.

### What this package does not own — and specifically must not

**It does not contain the registry, and it must never gain one.** This is the
package's defining constraint, not an accident of where the code sat.

A registry names every rule. Every rule depends on the engine. Put the two in
one crate and the dependency graph has a cycle that Cargo cannot express. So
the engine is handed a `RuleCatalog` and stays ignorant of which rules exist
and of how many there are.

The engine sizes its state from the catalogue at runtime, avoiding a type-level
dependency on the registry's cardinality. The state is built once per file,
not per node.

Also not here:

- **No rules.** Not one. They live in `feature/lint-*` packages.
- **No report.** `lint_report` calls this engine *with* a registry, so it
  cannot be core; it belongs with the feature that owns it.
- **No CLI.** Flags and exit codes are the composition root's.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-syntax` | The pass walks a `SyntaxTree`; `RuleContext` hands rules views, spans and dialect. |
| `paredit-core-semantics` | A rule that needs the binding or value table reads it from the context instead of rebuilding it — that seam is why the engine knows about semantics at all. |
| `thiserror` | Typed errors in the model, and `LintError` for the pass. Of the 134 registered rules, **four** are fallible, and all four fail the same way: they consult the whole tree, resolve an expression path, and it does not resolve. `anyhow::Result` said "this can fail for any reason", which is what hid that. See `src/error.rs`. |
| `proptest` (dev) | Properties over generated documents. |

## Public API

| Module | Principal items |
| --- | --- |
| `rule` | `LintRule` (the contract), `RuleEntry` (one registered rule), `RuleCatalog` (the set a run operates over) |
| `engine` | `collect_lint_outcomes`, `build_head_index`, `HeadIndex`, `RuleContext`, `RuleSink` |
| `model` | `LintFinding`, `LintOutcome`, `RuleFix`, `Fixability`, `Severity`, `RuleCategory`, `RuleMeta`, `HeadFilter`, `LintSummary`, `LintPolicy` |
| `policy` | `RuleSelection`, `RuleDialectScope`, `resolve_active_rules`, `summarize_lint_findings`, `lint_gate_violations`, `evaluate_lint_policy` |

`engine::collect_lint_outcomes` and the four `policy` functions all take a
`RuleCatalog` as their first argument. The root crate wraps each one with the
shipped catalogue, so callers there keep the shorter signatures.

`#[non_exhaustive]` is deliberately absent: every member is `publish = false`
and internal, and it would defeat the exhaustiveness checking the migration is
trying to gain (§9.4).

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| changing what a rule can ask for, or what it can report | the `LintRule` contract is here |
| making the single pass reach nodes it currently misses | dispatch and the head index are here |
| adding a field to a finding or a fix | the model is here |
| changing how `--rule`/`--category` resolve, or how the CI gate decides | policy is here |
| giving rules access to a new analysis without each rebuilding it | add it to `RuleContext` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding or editing a lint rule | rules live in `feature/lint-*`; this package must not name one |
| adding anything that enumerates rules | that is the registry, and it stays outside — see above |
| producing the lint report or its JSON | that is `feature/lint-report` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
