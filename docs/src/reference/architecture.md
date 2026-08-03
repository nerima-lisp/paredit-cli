# Architecture

`paredit-cli` is a Cargo workspace: a thin composition root plus 74 packages
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
              feature/*  (65 packages, mostly independent of each other)
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

Almost nothing, which is the point:

```text
src/
├── lib.rs, main.rs      entry points
├── lint/                the registry, and the pass that runs it
├── semantic_coverage.rs a development harness
└── presentation/        the clap tree, dispatch, and the protocol servers
```

A contract test walks `src/` and refuses anything else.

The lint `REGISTRY` is the canonical example of what *must* live here. It names
all 345 rules, and every rule depends on the engine; putting the registry in
either would be a cycle. So the engine takes a `RuleCatalog` as an argument and
never learns which rules exist, the rules never learn the registry does, and
the registry sits in the root reaching forty feature packages for their
`META` and `RULE`. That is the criterion: **a module that enumerates or
aggregates several features** belongs in neither core nor any one feature.

There is no `domain`, `application` or `infrastructure` module, and the names
are the reason. They used to hold 415 lines of `pub use`
re-exporting other packages, on the reasoning that they were "the public API's
namespace". Measured, 26 of those lines were referenced and the crate is
`publish = false`, so the namespace had no consumer outside this repository.
Worse, the names were an invitation: a directory called `domain` is where "just
put it here for now" goes, which is why seven report modules had accumulated in
one. Callers now name the package that owns the type, and `src/infrastructure`
— five lines, all re-export, zero consumers — is gone entirely; the
infrastructure layer is `packages/core/workspace`, and saying so twice only
made one of them a lie.

`paredit_cli::{dialect, sexpr}` still resolve, re-exported in `lib.rs` from
`paredit-core-syntax` directly.

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
| Infrastructure | `core/workspace` | Turns filesystems and workspace discovery into inputs the application layer can consume. There is no `src/infrastructure`; this is it. |
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

## Lint rules: one trait, one registry line, forty packages

The lint suite is the clearest example of the split's shape, and the most
frequently extended part of the tree.

`paredit-core-lint-engine` owns the mechanism and nothing else:

| Module | Role |
| --- | --- |
| `rule` | The `LintRule` trait, `RuleEntry`, and `RuleCatalog`. A rule declares which nodes it wants (`head_filter`) and what to say about one (`check`); it never walks the tree itself. |
| `model` | Vocabulary shared by every rule — `Severity`, `RuleCategory`, `Fixability`, `RuleMeta`, `LintFinding`, `RuleFix`. |
| `policy` | Dialect scope, rule selection and gate decisions: logic that needs no tree. |
| `engine` | The single pass, which walks the document once and dispatches each node to every rule whose `head_filter` matches. |

340 of the 345 shipped rules live in thirty-nine themed packages, split seven
ways. A fortieth, `feature/lint-custom`, holds no rules at all: it is the
pattern language and the second pass that run the rules a *project* writes for
itself.

The remaining five — `macro-variable-capture`, `macro-multiple-evaluation`,
`macro-parameter-reordering`, `macro-deep-quasiquote-nesting` and
`elisp-macro-missing-declare` — live in `feature/lisp-analysis` instead: not a
themed lint package, but the one that already owned the detection as the
standalone `inspect macro-hygiene` report. They follow the same
`rule/`/`domain.rs`/`usecase.rs`/`cli/` split as the seven older packages
below, for the same reason: the rules and the report share one detection
rather than each keeping its own. `rule/` is a directory rather than a file
there because one detection pass yields five distinct risks, and a project
must be able to deny, fail on, suppress and baseline each of them separately.

Six are split by the Lisp syntax they are about —
`feature/lint-{conditional,sequence,numeric,control-flow,form-shape,string-char}`.
`feature/emacs-lisp` is split by *dialect*: its rules are about Emacs
Lisp's own file conventions (`lexical-binding`, `;;;###autoload`, `defcustom`
options, the `cl.el` names Emacs 27 removed) rather than about S-expression
shape, so none of them has a Common Lisp counterpart to share a theme with.

Four —
`feature/lint-{performance,portability,safety,convention}` — are split by the
*kind of claim* the rule makes rather than by the syntax it reads: cost,
environment assumptions, what the form does to the world outside it, and what a
definition says about itself. Grouping those by operator would scatter each
argument across six packages.

The twelfth, `feature/lint-repl-debug`, is split by *provenance* rather than
syntax or claim: its eight rules all flag the same thing — an interactive
REPL session's leftovers (`print`/`trace`/`break`/`time`/`step`/... calls, a
`DEBUG`-marked `format`, a pasted transcript in a comment) accidentally
committed — which cuts across every one of the other four groupings and does
not belong in any of them.

Three — `feature/lint-{object-system,condition-system,iteration-flow}` — are
split by the *language subsystem* whose contract the rule encodes: CLOS's class
and method protocol (8 rules), the condition system's signalling, handling and
restart protocol (7), and the iteration macros' clause grammar — `loop`,
`dotimes`, `dolist` (6). Each of those is a self-contained CLHS chapter with
its own vocabulary and its own failure modes, and a rule in one is unreadable
without that chapter's rules around it. All 21 are Common Lisp only and
`ReportOnly`: each reports a judgment the tool cannot make for the author —
inserting a `call-next-method`, choosing a `:report` string, or reordering
`loop` clauses all change what the code *means*, not merely how it reads.

The last three — `feature/lint-{testing,concurrency,build-system}` — are split
by the *program-level concern* the code serves, which cuts across syntax and
across the CLHS: what a test has to do to be worth running (6 rules), what
state shared between threads requires (7), and what a system definition and
package declaration must say for a project to build and namespace itself (4).
A rule in any of the three reads ordinary forms — `let`, `defmethod`, a
function call — and is about the role those forms play, not their shape.

These three are not uniformly Common Lisp — though they are not the first group
that isn't, and it is worth being precise about what is actually new. Multi-
dialect rules already exist: `feature/emacs-lisp` is not Common Lisp at all,
and `lint-repl-debug`'s `leftover-print-debug` declares eight dialects.
`lint-testing`'s six rules span Common Lisp, Emacs Lisp and Clojure, because a
test framework's vocabulary is per-dialect and the same `deftest` spelling
means different things in two of them. Two of `lint-concurrency`'s seven —
`atom-swap-with-side-effect` and `future-promise-never-realized` — are
**Clojure only**. Rules scoped *away* from Common Lisp are not new either: ten
are already `EMACS_LISP_ONLY`. What these two are the first of is narrower —
rules scoped to a dialect for a construct Common Lisp has no counterpart for at
all, so there is nothing to generalize `atom`s, `future`s and `promise`s
*to*. All 17 are `ReportOnly`.

The seventh grouping is the four newest —
`feature/lint-{call-shape,documentation,contract-annotation,introspection}` —
split by *what the rule reads instead of the operator*. Every grouping above
keys on the form's head; these four do not. `lint-call-shape` (5 rules) reads
the size and nesting of an argument list rather than what is being called;
`lint-documentation` (4) reads the prose in a docstring or comment and checks it
against the code beside it; `lint-contract-annotation` (2) reads a *separate*
annotation form — Typed Racket's `(: name (-> …))` and Clojure's `:pre`/`:post`
— and compares it with the definition it describes; `lint-introspection` (3)
reads a name that will not exist until
run time, built by `intern` or looked up by `find-symbol`, which is precisely
the case no other rule can follow.

This group is the least Common-Lisp-centric in the tree, and it is where the
dialect matrix gains cases it did not have. `typed-racket-arity-mismatch` is
the first built-in rule scoped to **Racket** alone; the other of
`lint-contract-annotation`'s two is Clojure only, leaving **no** Common Lisp
rule in that package — which matters because Common Lisp is the
`RuleDialectScope` trait default, so a rule there that lost its scope override
would silently start walking every `.lisp` file.
`todo-fixme-no-attribution` goes the other way and
declares **all eleven** dialects, because a `TODO` with nobody's name on it
reads the same in every one. All 14 are `ReportOnly`, and 8 of them are tagged
`pedantic` — a threshold on parameter count or lambda nesting, and a house
style for docstrings and `TODO`s, are conventions a codebase either adopted or
did not, so `recommended` withholds them and `--preset pedantic` turns them on.

A rule declares its `dialect_scope`, and the dispatcher skips one whose scope
excludes the file's dialect before walking anything. `inspect capabilities`'
dialect matrix reads that same declaration, so a rule's standalone command
reports support for exactly the dialects the rule runs on.

Layout inside a package follows what a rule is *shared with*. The seven older
packages give each rule a directory holding `rule.rs` (what the registry
registers), `domain.rs` (the detection), `usecase.rs`, and `cli/` — because
each of those rules also has its own standalone `inspect <rule>` command, and
the split is what lets the command and the rule share one detection. The four
newer packages give each rule a single module: they ship as lint rules only,
reachable through `inspect lint --rule <name>`, so there is one consumer and
the three-file split would be indirection with nothing on the other end. The
seven after those —
`feature/lint-{repl-debug,object-system,condition-system,iteration-flow,testing,concurrency,build-system}`
— return to the directory layout, for the original reason: every one of their
rules also ships a standalone `inspect <rule>` command.

The four newest show that the directory layout and the standalone command are
two decisions rather than one. `feature/lint-{call-shape,introspection}` take
the directory *with* `cli/`, as before — 8 rules, 8 commands. But
`feature/lint-{documentation,contract-annotation}` take the directory *without*
it: their 6 rules are reachable only through `inspect lint --rule <name>`, and
the `rule.rs`/`domain.rs` split still earns its keep, because the detection is
the part worth testing apart from the phrasing. A rule's layout now says what
its *detection* needs; the presence of `cli/` says whether it also answers on
its own. The batch that introduced these four added 37 rules but only 19
commands — the other 18 come from those two packages and from new
single-module rules in `feature/lint-{safety,performance,portability}`. It
proposed 39; `lint-contract-annotation`'s `check-type-redundant-with-declare`
and `clojure-pre-referencing-percent` were dropped before merge once their
premises were checked against CLHS 3.3.1 and `clojure/core.clj` and refuted.
That package's README records both refutations.

The batch after that added 20 rules and 20 commands — every one of them takes
the directory *with* `cli/` — and only one new package. Sixteen of the twenty
went into `feature/lint-{form-shape,sequence,numeric,string-char}`, which
already existed: they are about the shape of a form, a sequence operation, a
numeric operation and a `format` control string respectively, which is what
those packages are for, so a new package would have been a second home for one
subject. The twentieth's package, `feature/lint-package-hygiene`, is new
because its subject — how a file *selects* the package its forms are read into
— had no home at all. Its README is worth reading before adding to it: it
records five rules that were asked for and deliberately **not** built, each
with the reason, and the most useful of those is
`package-nickname-shadows-existing-package`, which would duplicate `inspect
package-conflicts`. That report is a real gap — it carries no severity, no
`paredit:ignore` suppression and no place in aggregated `inspect lint` — but
the fix is a thin `rule.rs` delegating to its existing domain function, not a
reimplementation.

Three of the twenty are the first rules in these four older packages scoped
away from Common Lisp: `nested-get-chain` and `redundant-into-empty-collection`
are Clojure's `get-in` and `into`, and `division-result-precision-loss` is
Emacs Lisp's truncating integer `/`. Each needs an arm in `contract.rs`'s
`lint_rule_dialect_scope` and an entry in `DIALECT_SPECIFIC_REPORTS`, or the
dialect matrix claims a Common Lisp support the rule declines.

The batch after *that* is the first whose packages are entirely outside Common
Lisp rather than carving an exception out of it:
`feature/lint-{clojure-idiom,scheme-idiom}`, 4 rules and 4 commands each, all
eight with the directory-plus-`cli/` layout. Neither had a home — the Clojure
rules are about `with-open`, an inline `def`, the
`get-in`/`assoc-in`/`update-in` family and a spread `[…]` literal, and the
Scheme ones about `begin`, `let*`, `memq`/`assq` and the named `let`, none of
which any syntax-themed package could hold without becoming a second home for
its subject. `lint-scheme-idiom` is also the first package whose rules are
mostly `Fixability::Fixable`: three of its four repairs rewrite a single head
symbol and the fourth copies an inner span verbatim, so spacing and comments
survive.

Those four are what made `DIALECT_SPECIFIC_REPORTS` grow a second dimension.
Every scoped rule before them named exactly *one* dialect, and
`contract.rs`'s companion test had hardened that accident into
`assert_eq!(supported, 1)`. Three of the Scheme rules declare
`[Scheme, Racket]`, because `begin`, `let*` and the named `let` read
identically in both, so the test now asserts a *proper subset* of the ten
dialects instead. Only `scheme-memq-assq-literal-key` is Scheme alone, and for
a reason worth keeping: Racket's `memq` is `eq?`-based too, but Racket
specifies the two cases R7RS 6.4 leaves open — fixnums compare `eq?` by
guarantee and characters have been normatively `eq?` since 9.0.0.10 — so every
finding there would complain about code the language promises will work.

**`REGISTRY` is in neither.** It names all 320 rules, and every rule depends on
the engine, so putting it in the engine or in a rule package would be a cycle.
It sits in the root crate, and the engine receives a `RuleCatalog` as an
argument — which is why the engine can be a package at all.

**Custom rules cannot be in `REGISTRY` either**, for a different reason:
`RuleCatalog` holds `&'static [RuleEntry]` so the four derived arrays can be
computed at compile time, and a rule read from a file at startup has no
`'static` lifetime to offer. They run as a second pass whose findings are
merged into the report, and the merge is two functions in
`src/presentation/cli/lint_report/workflow.rs`. The two passes share the
finding type — so every output mode renders both — and nothing else.

Adding a rule is three places of *design* and several more of bookkeeping. The
design:

1. Add `packages/feature/lint-<theme>/src/your_rule/` with `rule.rs` and
   `domain.rs`.
2. Add one `RuleEntry::new(...)` line to `REGISTRY` in
   `src/lint/registry/mod.rs`, and bump `RULE_COUNT` with it.
3. Add one integration test in `tests/cli/lint_report.rs`, or a fixture pair
   under `tests/fixtures/lint_golden` for the golden test.

The bookkeeping is deliberate — the suite's shape is pinned so that a rule
cannot appear or vanish unremarked — but it is not free, and it is what makes
"three places" misleading. Adding the five `inspect macro-hygiene` rules
touched sixteen files. Budget for all of these:

- The const assertions beside `RULE_COUNT` in `src/lint/registry/catalog.rs`:
  `fixable_count()`, `warning_count()`, `EXPERIMENTAL_RULES`, and
  `PEDANTIC_RULES.len()`. These are compile errors, so they cannot be missed —
  but each one has to be recomputed, not merely incremented.
- The pinned counts in the integration tests: `rule_count` and the
  warning-severity tally in `tests/cli/lint_report.rs`, and the rule count in
  the docstring of `tests/cli/determinism_contract.rs`. These fail at test
  time, and the prose beside them goes stale silently.
- The goldens under `tests/fixtures/lint_golden/expected/`. Every rule appears
  in each fixture's per-rule tally and in the SARIF `rules` array, so a new
  rule rewrites all twelve golden files even when it finds nothing.
  `UPDATE_LINT_GOLDEN=1` regenerates them; read the diff rather than accepting
  it.
- The rule counts written into prose: `docs/src/reference/api.md` and this
  file. Nothing checks these, which is exactly why they drift.

A rule id is public API from the moment it is released, so settle on the name
before any of the above: renaming one afterwards means a breaking change to
every `lint.deny`, `lint.fail-on`, baseline entry and `paredit:ignore` comment
in every downstream project.

## Semantics: read-only tables beside the tree

`packages/core/semantics` lets a rule reason about what code *means* rather than
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

**A port names its adapter's error as an associated type.**

```rust
pub trait DefinitionSourcePort {
    type Error: Into<CliError>;
    fn load(&mut self, file: &Path) -> Result<LoadedDefinitionSource, Self::Error>;
}
```

These methods returned `anyhow::Result` until the typed-error pass, justified
by the port's whole purpose: the use case must not know what an adapter can
fail with. That reasoning is right and `anyhow::Error` was the wrong way to say
it — it does not express "some error I do not name", it expresses "no error
type at all", and the failure's classification went with it. An associated type
says the intended thing in the type system. The `Into<CliError>` bound is the
one requirement, because whatever an adapter fails with has to be reportable at
the CLI boundary with an error code.

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
   [agent interface](../guide/agents.md) for the code table).

Keeping request conversion, rendering, and gate-to-exit mapping here — and
everything else in the application and domain layers — is what lets the same
report logic serve both a human `--output text` reader and a machine
`--output json` consumer without duplication.

### The failure a command returns

A command entry point returns `CommandResult` — `Result<(), CommandFailure>`
— and `CommandFailure` has exactly two variants:

| Variant | Means |
| --- | --- |
| `Error(CliError)` | the command could not do its work |
| `Gate(GateFailure)` | it *did* its work, printed its report, and a requested `--fail-on-*` gate tripped |

Those are different answers and they earn different exit codes (1 and 3), so
`diagnosis::classify` is a total `match` over the pair. It used to be a chain
of `downcast_ref` probes against an `anyhow::Error` ending in
`.map_or(ErrorCode::Internal, ...)`, which meant a failure nobody had written a
probe for was reported to the caller as `internal.unclassified` — "a defect in
this tool" — for perfectly ordinary refusals. A closed sum makes that a compile
error instead.

The set of error codes is closed and documented; the set of *feature* errors is
open, because `CliError` naming all 29 feature packages would invert the
dependency direction. `FeatureRefusal` bridges the two: a feature converts its
own rich error at its `cli/` boundary and **must** supply the code it earns.
The `source()` chain flattens into the message there — the last point at which
anything reads it — while the classification stays a type.

## How the layers map to the three namespaces
## How the layers map to the namespaces

The [command model](api.md) — `inspect`, `edit`, `refactor`, `query`,
`fix`, `migrate` — is a presentation-level grouping. Underneath, an `inspect`
report and a `refactor` plan are both application use cases over the same
domain `SyntaxTree`; the namespace only reflects whether the command writes
and what the caller was trying to do. This is why a report and the refactor
that consumes it always agree on paths, spans, and symbol identity: they share
the domain, not just a serialization format.

The three added namespaces make the point sharply, because each is a *new
address* onto machinery that already existed rather than new machinery:

| Namespace | What it exposes | Where the logic lives |
| --- | --- | --- |
| `query` | the pattern language, at workspace scope | `paredit_core_syntax::selector::{pattern, matcher, rewrite}` |
| `fix` | the lint auto-fixer | the same engine `inspect lint --fix` runs, called with the arguments that spelling would have produced |
| `migrate` | ordered, dialect-scoped recipes | `selector::rewrite` again, sequenced by `paredit-feature-migrate` |

`selector::rewrite` sits in **core**, not in `paredit-feature-query`, for one
concrete reason: `paredit-feature-migrate` needs it too, and a feature package
depending on a feature package is the dependency direction this split exists
to prevent.

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
a presentation adapter. The [development guide](../project/development.md) covers the
verification gate that keeps these boundaries — and the documentation that
describes them — honest.
