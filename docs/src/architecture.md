# Architecture

`paredit-cli` is a Cargo workspace: a thin composition root plus 24 packages
under `packages/core/` and `packages/feature/`. Knowing which package owns a
thing is the fastest way to know where a change belongs.

## The two kinds of package

| | Owns | Depends on |
| --- | --- | --- |
| `packages/core/*` | Vocabulary every feature shares: parsing, semantics, editing primitives, the lint engine, workspace discovery, CLI I/O conventions | Other core packages only |
| `packages/feature/*` | One user-facing capability, whole: its rules, its orchestration, its subcommand | Core, and occasionally another feature |
| the root crate | The composition root: the `clap` command tree, the dispatch `match`, and the lint `REGISTRY` | Everything |

```text
core/syntax ──▶ core/semantics ──▶ core/edit ──▶ core/cli
     └──▶ core/workspace          core/lint-engine ──┘
                    │
                    ▼
              feature/*  (24 packages, mostly independent of each other)
                    │
                    ▼
              paredit-cli  (command tree, dispatch, REGISTRY)
```

The direction is enforced by `Cargo.toml`, not by convention. A core package
that names a feature fails a contract test; so does `clap` appearing outside a
`cli` module.

## One feature is one directory

Inside a feature package the layers survive as **names**, not directories:

```text
packages/feature/similarity/src/
├── form_similarity.rs              shared within the package
├── similarity_report/
│   ├── domain/                     the rules
│   ├── usecase/                    orchestration behind a source port
│   └── cli/                        args, workflow, render
└── duplicate_report/
    ├── domain.rs
    ├── usecase.rs
    └── cli/
```

This is the point of the split. Changing one feature means opening one
directory, not three trees. **Do not create `domain/`, `application/` or
`presentation/` directories at the top level of a package** — that reproduces
the old problem one level down. A slice grows a subdirectory per layer only
when that layer has more than one file, and a slice need not have all three.

## What the root crate still owns

`src/{domain,application,infrastructure,presentation}` remain as the public
API's namespace — `paredit_cli::domain::sexpr` still resolves — but they hold
re-exports rather than code. A contract test enforces that, with a short
allowlist for the **composition root**: modules that enumerate or aggregate
several features and therefore belong in neither core nor a feature.

The lint `REGISTRY` is the canonical example. It names all 134 rules, and every
rule depends on the engine; putting the registry in either would be a cycle. So
the engine takes a `RuleCatalog` as an argument and never learns which rules
exist, the rules never learn the registry does, and the registry sits in the
root reaching six packages for their `META` and `RULE`.

The same test identifies anything else that aggregates features: a module
naming three features' capabilities is composition root regardless of its size.

## Where the detail lives

This document owns **relationships between packages**. Each package's
`README.md` owns **its own boundary** — what it is for, what it refuses, why
each dependency exists, and where a change of a given kind belongs. The two do
not repeat each other. When you want to know what a package does, read its
README; when you want to know how packages fit together, read this.

## Layers, as names inside a slice

| Layer | Where it lives now | Responsibility |
| --- | --- | --- |
| Domain | `<slice>/domain` | Core Lisp parsing, dialect detection, and semantic refactoring rules. Independent of CLI delivery and filesystems. |
| Application | `<slice>/usecase` | Orchestrates typed domain operations into agent-facing reports, plans, and refactor workflows. |
| Infrastructure | `core/workspace` | Turns filesystems and workspace discovery into inputs the application layer can consume. |
| Presentation | `<slice>/cli` | Maps commands, flags, and output modes onto application services; renders reports and chooses exit codes. |

Within a slice the direction is unchanged — `cli` calls `usecase` calls
`domain`, never the reverse — but the rule is now mechanical rather than observed. A slice's `domain.rs` cannot
reach its `cli/` without saying so, `clap` outside a `cli` path fails a contract
test, and a feature dependency in a core package fails another. What used to be
a convention the module graph happened to respect is now something the crate
graph refuses to compile.

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

## Lint rules: one trait, one registry line, six packages

The lint suite is the clearest example of the split's shape, and the most
frequently extended part of the tree.

`paredit-core-lint-engine` owns the mechanism and nothing else:

| Module | Role |
| --- | --- |
| `rule` | The `LintRule` trait, `RuleEntry`, and `RuleCatalog`. A rule declares which nodes it wants (`head_filter`) and what to say about one (`check`); it never walks the tree itself. |
| `model` | Vocabulary shared by every rule — `Severity`, `RuleCategory`, `Fixability`, `RuleMeta`, `LintFinding`, `RuleFix`. |
| `policy` | Dialect scope, rule selection and gate decisions: logic that needs no tree. |
| `engine` | The single pass, which walks the document once and dispatches each node to every rule whose `head_filter` matches. |

The 134 rules live in six themed packages, split by the Lisp syntax they are
about: `feature/lint-{conditional,sequence,numeric,control-flow,form-shape,string-char}`.
Each rule is one directory holding `rule.rs` (what the registry registers),
`domain.rs` (the detection), `usecase.rs`, and `cli/` (its own `inspect`
subcommand).

**`REGISTRY` is in neither.** It names all 134 rules, and every rule depends on
the engine, so putting it in the engine or in a rule package would be a cycle.
It sits in the root crate, and the engine receives a `RuleCatalog` as an
argument — which is why the engine can be a package at all.

Adding a rule touches exactly three places:

1. Add `packages/feature/lint-<theme>/src/your_rule/` with `rule.rs` and
   `domain.rs`.
2. Add one `RuleEntry::new(...)` line to `REGISTRY` in
   `src/domain/lint/registry/mod.rs`. `RULE_COUNT`'s const assertion means
   forgetting this is a compile error, not a silently shorter report.
3. Add one integration test in `tests/cli/lint_report.rs`, or a fixture pair
   under `tests/fixtures/lint_golden` for the golden test.

## Semantics: read-only tables beside the tree

`src/domain/semantics` lets a rule reason about what code *means* rather than
how it is spelled. It is why `zero-divisor` flags `(let ((z 0)) (/ x z))` and
not just `(/ x 0)`.

Nothing here rewrites the tree. Formatting survives a refactor because every
edit is a byte-span replacement over untouched source, and that discipline only
holds while the tree stays authoritative — so the analyses hang beside it as
side tables keyed by `NodeKey`, never as annotations on it.

| Context | Answers |
| --- | --- |
| `binding` | Which binding does the name at this position mean? Built once per file, from the same knowledge `lexical_scope` uses to answer the inverse question. |
| `value` | What does this expression provably evaluate to? |
| `typing` | What type is this, at a coarse CLHS granularity? Common Lisp only. |
| `project` | Which package owns this symbol, so `app:run` and `test:run` are two things? |

Each context splits into `model` (vocabulary), `policy` (dialect tables), and
`service` (the pass that builds a table). They stack — values need bindings,
types need values — and link downward **by id, never by borrow**: a
`ValueTable` holding a `&BindingTable` would make them one self-referential
struct. A rule reaches them through `RuleContext`, which builds each on first
use, so a run whose rules ask for none pays for none.

Two rules hold throughout, and both cost deductions on purpose:

- **A fact is recorded only when it is provable.** Anything uncertain is absent
  rather than guessed, because a rule that trusts a wrong `Known` reports a bug
  in working code.
- **An unknown head is opaque.** A macro can expand into an assignment that
  appears nowhere in the source, so propagation stops at any head whose
  semantics are not registered. Ordinary function calls and standard control
  forms are exempt — a function cannot reach the caller's lexical environment
  at all, and a control form evaluates its subforms where they are written, so
  any assignment inside is visible.

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
| New lint rule | `domain/lint/rules` plus one `registry` line |
| New static fact about values, types, or bindings | `domain/semantics` |
| New report, plan, or multi-file workflow orchestration | `application` |
| New way to discover or read sources | `infrastructure` |
| New command, flag, output format, or exit-code mapping | `presentation` |

When a change spans layers, add it from the inside out: model it in the domain,
orchestrate it in an application use case behind a port, then expose it through
a presentation adapter. The [development guide](development.md) covers the
verification gate that keeps these boundaries — and the documentation that
describes them — honest.
