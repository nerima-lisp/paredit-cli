# paredit-feature-lint-package-hygiene

Lint rules for Common Lisp package declaration and selection hygiene.

## Responsibilities

One rule about how a file *selects* the package its forms are read into.

| Rule | Flags |
| --- | --- |
| `package-circular-in-package-chain` | a top-level `(in-package …)` that re-enters a package the same file had already left |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

**On the name.** `package-level-shadowing` is the only other rule whose name
starts with `package-`, and the two are unrelated, so they will sort adjacently
in `--rule` listings for no reason. The name is kept anyway: this rule *is*
about package hygiene, the adjacency is therefore not misleading, and a rule
name is a compatibility surface once it ships. If a `package-*` family grows
here, revisit then rather than per-rule.

`in-package` is a *reader-time* switch. Every form after it is read — and so
every unqualified symbol in it is interned — into that package. A file that
goes `A → B → A` therefore has two disjoint regions of `A` with a region of `B`
wedged between them, and a symbol spelled the same way in the `B` region is a
different symbol from the one in the `A` regions. The load order of the file's
own definitions now depends on where the switches fall rather than on where the
definitions do, which is a hazard that no amount of reading one form can reveal.

The rule is `Fixability::ReportOnly`. Merging the two regions of `A` means
*moving forms*, and which region should absorb the other — and whether the `B`
region depends on the first `A` region having run — are questions a rewrite
cannot answer.

### What it deliberately does not flag

- **A re-entry of an ambient package.** `CL-USER`, `COMMON-LISP-USER`, `CL`,
  `COMMON-LISP` and `KEYWORD` exist in every image without a `defpackage`, and
  bouncing through `CL-USER` between two package declarations is a normal,
  correct single-file-library shape. The exempt set is the same one
  `paredit_feature_project_analysis::undefined_package_report` declares, for the
  same reason.
- **A non-top-level `in-package`.** CLHS 11.1.2.2 requires `in-package` to
  appear at top level for its compile-time effect; one nested inside
  `eval-when`, `progn` or a function body does not switch the reader for the
  following top-level forms, so it is not part of the chain.
- **Quoted data.** `'(in-package :a)` is a list of two symbols.
- **A reader-conditional switch.** `#+sbcl (in-package :a)` parses as a single
  opaque atom in this codebase, so it never enters the chain at all. The
  build-dependent chain that results cannot be read statically, and guessing at
  it would report on faith.
- **Adjacent repetition.** `(in-package :a)` twice in a row with no other
  package between them is redundant, not circular; nothing was left and
  re-entered. That is a different complaint and this rule does not make it.

## What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No `package-nickname-shadows-existing-package` rule.** Asked for, and
  deliberately **not implemented**: a `:nicknames` entry that collides with
  another package's primary name is
  `inspect package-conflicts`'s subject by name
  (`packages/feature/package/src/package_conflict_report/domain.rs`, whose module
  doc opens with "a nickname that collides with another package's primary
  name"). It already covers the same-file case *and* the cross-file case, so a
  rule here would see strictly less and report the same spans under a second
  name.

  The one real gap is that `package-conflicts` is a standalone report
  (`Commands::PackageConflicts`, `src/presentation/cli/command.rs`) and is *not*
  in `REGISTRY` — so the analysis exists but carries no severity, no
  `paredit:ignore` suppression, and no place in aggregated `inspect lint`.
  Closing that gap is worth doing and is **not** this package's job to do by
  reimplementation: the right shape is a thin `rule.rs` delegating to the
  existing report's own domain function, the adapter pattern
  `feature/lint-object-system`'s `slot_value_bypasses_accessor/rule.rs` already
  uses. That needs a `paredit-feature-package` dependency, which needs a line in
  `tests/cli/feature_dependency_contract.rs` — a file this package's author was
  scoped out of, and which must change in the same commit as the manifest.

  Reimplementing the detection here instead was considered and rejected: this
  project has already shipped three independent rule/report pairs answering one
  question, and a reviewer found every one of them had diverged in *both*
  directions. Two answers to one question is the defect, not the fix.
- **No `package-reexport-without-alias-doc` rule.** Asked for, and
  deliberately **not implemented**: the defect is not expressible. Neither
  `cl:defpackage` nor `uiop:define-package` can export an imported symbol under
  a *different* name. `(:import-from #:cl #:car)` with `(:export #:head)` does
  not rename anything — it interns a brand-new, unbound `HEAD` in the new
  package, which `(eq 'pkg:head 'cl:car)` answers `NIL` to. `uiop`'s
  `:reexport` is documented as exporting "symbols with the same name". There is
  no rename to be undocumented.
- **No `package-export-of-unbound-symbol` rule.** Asked for, and deliberately
  **not implemented**: a package is routinely implemented across several files,
  so "no `defun` for this export *in this file*" is the normal state of every
  correct `package.lisp`. Exporting a symbol with no definition is also legal
  and sometimes deliberate, and the generated names of `defstruct`, a
  `defclass` `:accessor`, a `setf` expander and any user `def…` macro are
  invisible to a syntactic file-local check. The sound form of this question is
  project-wide with an opt-in gate, which is exactly what
  `inspect unused-exports` and `inspect undefined-packages` already are.
- **No `package-use-clause-nothing-consumed` rule.** Asked for, and
  deliberately **not implemented**: the canonical layout puts `defpackage`
  alone in `package.lisp`, so "this file references none of the `:use`d
  package's exports" is true of essentially every correct project.
  `inspect use-widening` already reports a sound superset — every `:use`d
  package, because `:use` imports the whole exported table — without having to
  guess at consumption.
- **No `package-single-exported-symbol` rule.** Asked for, and deliberately
  **not implemented**: a package with one export is a completely normal thing
  to write, and the obvious hardening (require the package to also define many
  internal symbols) selects for *facade* packages, which is good design rather
  than a defect.
- **No `defpackage-without-in-package` rule.** That is
  `feature/lint-build-system`'s, and it is the complementary question: a file
  that never enters a package at all, rather than one that enters too many.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms and on Common Lisp operator spelling. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for the rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── support.rs           quote-aware traversal and the top-level in-package chain
└── <rule>/
    ├── rule.rs          META, RULE, the head filter: what the registry registers
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs`'s quote machinery is a deliberate **copy** of
`feature/lint-condition-system`'s (by way of `feature/lint-build-system`'s
span-local refinement of it), not a dependency on it: two packages of lint rules
should not couple, and the semantics are the part worth sharing. Two independent
counters, because `'` and `` ` `` are not the same thing — a comma inside `'(…)`
is a comma *character* in literal data, so a single depth counter gets `'(a ,X)`
wrong, and a node one level inside a quote is still data, so a node-local
`reader_prefixes` check is not enough either. All five shapes are pinned by
tests in `support.rs` and again in the rule's own tests and in the engine pass.

## Cost

The rule is `HeadFilter::Heads` — never `AllNodes`, never `WholeTree`. The
`clean/forms/*` benchmarks lint files with zero findings, so the per-file cost
of a rule that matches nothing is exactly what they measure, and those fixtures
contain no `in-package` at all: this rule's `check` is never called on them, and
adding it to the `in-package` bucket of the head index costs nothing anywhere
else either.

`WholeTree` was considered, since the chain is a property of the file and
`feature/lint-build-system`'s `defpackage-without-in-package` takes exactly that
exception. It was declined: `WholeTree` rules are dispatched on *every* file and
would pay a byte scan over every clean benchmark file, where `Heads` pays
literally nothing there. `defpackage-without-in-package` has to be `WholeTree`
because it is about an *absence* and so has no head to anchor on; this rule is
about a `in-package` form that is present, and reports at it.

What each invocation does pay is one scan of the *top level* — a node-id lookup
and a span read per top-level form, no allocation, no `ExpressionView` — to
recover the chain the file's `in-package` forms form. A file has a handful of
top-level `in-package` forms, so that is a small constant number of linear
passes and is linear in the file overall; `docs`-free proof lives in the
`measurement` test in `src/lib.rs`, which pins the doubling ratio.

`SyntaxTree::root_view` is never called. It deep-materializes the whole document
— a `Vec` per node and a `String` per atom, uncached, on every call — and three
rules elsewhere in this codebase were found calling it once per finding. The
top-level scan reads spans through `select_path`, which walks node ids only, and
`is_unevaluated_at` materializes at most the one top-level form containing its
target. No rule here calls
`RuleContext::binding_table`/`value_table`/`type_table`, and none touches
`RuleContext::scratch_cache`, which holds one type per file's pass and is
already claimed by `feature/lint-repl-debug`.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about `defpackage`/`define-package`/`in-package` hygiene | it is a new slice here, plus one line in the root's REGISTRY |
| changing what the rule flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms the rule is shown | that rule's `rule.rs` head filter |
| changing which packages count as ambient | `support.rs`'s `AMBIENT_PACKAGES` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| detecting package name/nickname collisions | that is `inspect package-conflicts` |
| deciding what a package ought to export, or whether an export is read | that is `inspect api-surface` / `inspect unused-exports` |
| finding `:use`/`:import-from` loops between packages | that is `inspect package-cycles` |
| flagging a blanket `:use` | that is `inspect use-widening` |
| flagging a file that enters *no* package | that is `feature/lint-build-system` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
