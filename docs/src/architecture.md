# Architecture

`paredit-cli` is organised as four layers with a strict, one-directional
dependency rule. The layers are the top-level modules of the crate
(`src/lib.rs`): `domain`, `application`, `infrastructure`, and `presentation`.
Understanding this shape is the fastest way to know where a change belongs.

## Layers and dependency direction

| Layer | Module | Responsibility |
| --- | --- | --- |
| Domain | `src/domain` | Core Lisp parsing, dialect detection, and semantic refactoring rules. Independent of CLI delivery and filesystems. |
| Application | `src/application` | Orchestrates typed domain operations into agent-facing reports, plans, and refactor workflows. |
| Infrastructure | `src/infrastructure` | Turns filesystems and workspace discovery into inputs the application layer can consume. |
| Presentation | `src/presentation` | Maps commands, flags, and output modes onto application services; renders reports and chooses exit codes. |

Dependencies point in one direction only. The domain is the stable core; the
presentation layer is the only place that knows about all the others:

```text
presentation ──▶ application ──▶ domain
      │                             ▲
      └────────▶ infrastructure ────┘
```

The rule is enforced by the module graph, not just by convention:

- `domain` imports no other layer.
- `application` imports only `domain`.
- `infrastructure` imports only `domain`.
- `presentation` composes all three.

If a `use crate::presentation` or `use crate::infrastructure` appears inside
`domain` or `application`, the boundary has been violated — that direction
never exists in a healthy tree.

## Domain: typed values, not primitives

The domain closes invalid states at the type level rather than validating
primitives at call sites. Byte positions are `ByteOffset`/`ByteSpan`, tree
addresses are `ExpressionPath`, symbol tokens are `SymbolName`, and a parsed
document is a `SyntaxTree` aggregate that stays internally consistent. Report
and decision types keep their fields private and expose semantic getters, so a
value like a similarity ratio (`0.0..=1.0`, finite) or a refactor plan's
automation decision cannot be constructed in a contradictory state.

Prefer this discipline when extending the domain: a validated newtype or a
semantic enum (`ReportLimit::{Complete, Limited(NonZeroUsize)}`,
`SimilarityGateDecision`) over a bag of correlated `bool`/`usize` fields.
Derive redundant presentation values (booleans, counts) at the serialization
boundary instead of storing them.

## Lint rules: one trait, one registry line

`src/domain/lint` is a concrete example of the domain discipline above, worth
knowing on its own because it is the most frequently extended part of the
domain. `src/domain/lint_report.rs` is the stable façade the application and
CLI layers import — report types, catalogue constants (`RULES`, `RULE_DOCS`,
`FIXABLE_RULES`, `WARNING_RULES`), and the two entry points that lint one
parsed file. Underneath it:

| Submodule | Role |
| --- | --- |
| `rule` | The `LintRule` trait. A rule declares which nodes it wants (`head_filter`) and what to do with one (`check`); it never walks the tree itself. |
| `model` | Vocabulary shared by every rule and the façade — `Severity`, `RuleCategory`, `Fixability`, `RuleMeta`, `LintFinding`, `RuleFix`. |
| `policy` | Dialect scope, rule selection, and gate decisions — logic that needs no tree. |
| `engine` | The single pass: walks the document once, dispatching each node to every rule whose `head_filter` matches it. |
| `registry` | `REGISTRY`, the one array every rule is listed in. The catalogue constants the façade re-exports are derived from it at compile time. |
| `rules` | One file per rule (`rules/redundant_apply.rs`, `rules/car_reverse.rs`, …): a `META: RuleMeta` constant plus a `LintRule` impl. A rule that auto-fixes attaches a `RuleFix` from `check` — the fix lives with the rule, not in the CLI layer. |

Adding a rule touches exactly three places:

1. Add `src/domain/lint/rules/your_rule.rs` with a `META` constant and a
   `LintRule` implementation.
2. Add one `RuleEntry::new(...)` line to `REGISTRY` in
   `src/domain/lint/registry/mod.rs`.
3. Add one integration test in `tests/cli/lint_report.rs` (or a fixture pair
   under `tests/fixtures/lint_golden` for the golden test in
   `tests/cli/lint_report_golden.rs`).

No parallel arrays to keep in sync and no separate fix implementation to wire
into the CLI: `--fix`, `--fix-plan`, and `--diff` all read the same `RuleFix`
the rule itself produced.

## Application: use cases behind source ports

Each non-trivial CLI workflow is an application **use case** that owns the
whole orchestration — discovery, decoding, parsing, analysis, gate precedence,
and error typing — and depends on the outside world only through a **source
port** trait it defines itself. The recurring shape is *request in, plan out*:

```text
Request (input DTO)
   │
   ▼
use case ──uses──▶ SourcePort (trait, defined in application)
   │
   ▼
Plan (output aggregate: report + inventory + typed errors + gate decision)
```

Three ports carry the pattern today:

| Use case | Source port | Plan / output |
| --- | --- | --- |
| `usecase::similarity_report::workflow` | `SimilarityReportSourcePort` | `SimilarityReportPlan` |
| `usecase::workspace_report::workflow` | `WorkspaceReportSourcePort` | `WorkspaceReportPlan` |
| `usecase::remove_definition` | `DefinitionSourcePort` | edit plan + write policy |

Because the port is an interface, the use case is filesystem- and
CLI-agnostic: tests drive it with an in-memory adapter, while production wires
in the real one. A port models *discover-before-load* explicitly — for
example `SimilarityReportSourcePort` resolves each file's dialect during
`discover` and returns bytes from `load`, so dialect is never smuggled
alongside a failed read. Adapter state or ordering failures return through
`Result`; they never panic.

The `Plan` an application use case returns is the contract with presentation:
it holds the domain report, a discovery inventory, per-file typed errors, and a
single computed gate decision. Presentation reads the plan; it never
re-derives the decision.

## Infrastructure: discovery adapters

`src/infrastructure/workspace` implements source discovery: it walks directory
roots, applies hidden/generated/symlink/exclude filters, and yields the file
set the application ports request. `fs_identity` captures file identity for the
apply-time "changed on disk" guard. Infrastructure depends on the domain (for
dialect types) and nothing above it.

## Presentation: adapters, rendering, exit codes

`src/presentation/cli` is a thin edge. For each workflow it:

1. Converts CLI arguments into an application `Request`.
2. Implements the use case's source port (e.g. `CliSimilarityReportSource
   impl SimilarityReportSourcePort`) by delegating to the infrastructure
   `discover_workspace_files` / `WorkspaceDiscovery` adapter.
3. Calls the use case and renders the returned `Plan` as text or JSON.
4. Maps the plan's gate decision to a process exit code (see the
   [agent interface](agents.md) for the code table).

Keeping request conversion, rendering, and gate-to-exit mapping here — and
everything else in the application and domain layers — is what lets the same
report logic serve both a human `--output text` reader and a machine
`--output json` consumer without duplication.

## How the layers map to the three namespaces

The [command model](commands.md) — `inspect`, `edit`, `refactor` — is a
presentation-level grouping. Underneath, an `inspect` report and a `refactor`
plan are both application use cases over the same domain `SyntaxTree`; the
namespace only reflects whether the command writes. This is why a report and
the refactor that consumes it always agree on paths, spans, and symbol
identity: they share the domain, not just a serialization format.

## Where a change belongs

| Change | Layer |
| --- | --- |
| New parsing rule, dialect capability, or refactor safety check | `domain` |
| New report, plan, or multi-file workflow orchestration | `application` |
| New way to discover or read sources | `infrastructure` |
| New command, flag, output format, or exit-code mapping | `presentation` |

When a change spans layers, add it from the inside out: model it in the domain,
orchestrate it in an application use case behind a port, then expose it through
a presentation adapter. The [development guide](development.md) covers the
verification gate that keeps these boundaries — and the documentation that
describes them — honest.
