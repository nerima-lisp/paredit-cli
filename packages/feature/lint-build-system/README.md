# paredit-feature-lint-build-system

Lint rules for ASDF system definitions and package declarations.

## Responsibilities

Four rules about the two forms that decide how a Common Lisp project is *built*
and *namespaced* — `defsystem` and `defpackage` — plus the one `defmethod` that
customizes the build.

| Rule | Flags |
| --- | --- |
| `asdf-system-missing-version` | a *primary* `defsystem` (a name with no `/`) that declares no `:version` |
| `asdf-self-referential-depends-on` | a `:depends-on`/`:defsystem-depends-on`/`:weakly-depends-on` entry naming the enclosing system itself |
| `defpackage-without-in-package` | a file that declares a package, defines things, and never enters it with `in-package` |
| `asdf-perform-without-call-next-method` | a *primary* `perform` method on `load-op`/`compile-op` and a standard component class whose body never calls `call-next-method` |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

**`asdf-system-missing-version` carries `RuleTag::Pedantic`, so it is off under
`--preset recommended` and `--preset minimal` and only runs under `--preset
pedantic` or an explicit `--rule`.** A project that versions its systems out of
band, or that has one application system nobody depends on, is not making a
mistake; a rule that fires on every such `.asd` is noise. The other three rules
are untagged and run under every preset.

Every rule is `Fixability::ReportOnly`. What the version should be, which
package the file meant to enter, whether the dependency should be deleted or
renamed, and where in a method body the `call-next-method` belongs are all
questions a rewrite cannot answer.

Three of the four rules are `HeadFilter::Heads` — never `AllNodes`. The
`clean/forms/*` benchmarks lint files with zero findings, so the per-file cost
of a rule that matches nothing is exactly what they measure; a rule here that
matched every node would be paid for on every file in the corpus. No rule in
this package calls `RuleContext::binding_table`/`value_table`/`type_table`:
none of them needs a semantic pass, and asking for one rebuilds a whole-file
analysis.

`defpackage-without-in-package` is the one exception, and it is `WholeTree`.
"A file with definitions but no `in-package`" is a question about an *absence*,
and its answer is a property of the file rather than of any node. Under `Heads`
the rule was dispatched once per `defpackage` and ran the same whole-file walk
each time, discarding all but one result — worst on a `package.lisp`, the file
shape with the most declarations and the least reason to look past the first.
`WholeTree` makes it one call per file, and a free one: the dispatcher
materializes the root view unconditionally and hands that same view to
`WholeTree` rules, where under `Heads` the rule called `root_view()` itself and
rebuilt the whole document on each call. What it pays instead is being
dispatched on files with no package declaration at all, and a byte scan for
`defpackage`/`define-package` settles those before any walk. Its `rule.rs`
documents the ordering.

## What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No `asdf-component-file-mismatch` rule, and no `.asd` path resolution.**
  Asked for, and deliberately **not implemented**: `packages/core/workspace`'s
  `parse_asdf` already resolves every `:file`/`:module` component against the
  `.asd`'s directory, honours `:pathname` and ASDF's default-extension rule,
  and records the ones that are missing; `inspect sources` prints them. A lint
  rule that re-did that would be a second answer *and* the first filesystem
  access ever performed inside `LintRule::check`, which is a read-only,
  deterministic, tree-only contract today.
- **No `defpackage-nicknames-collision` rule.** Asked for, and deliberately
  **not implemented**: a `:nicknames` entry colliding with another package's
  name or nickname — in one file or across several — is
  `inspect package-conflicts`'s subject
  (`packages/feature/package/src/package_conflict_report/`), which already covers
  both collision shapes and the same-file case by name. A rule here would report
  the same spans under a second name.
- **No `unexported-public-looking-symbol` rule.** Asked for, and deliberately
  **not implemented**: which definitions *ought* to be exported is
  `inspect api-surface`'s and `inspect unused-exports`' territory, and
  `api-surface` has a test locking in the decision that a definition without an
  export is simply not part of the surface. "Looks public" is a convention, not
  a property of the source; a rule guessing at it would fire on most of a normal
  file.
- **No `:around` method rule.** `around-method-missing-call-next-method` in
  `feature/lint-object-system` is generic-agnostic and qualifier-gated to
  `:around`. `asdf-perform-without-call-next-method` skips *every* qualified
  method precisely so the two never report on one form.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms, on Common Lisp operator spelling, and on the shared `definition` classifier. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── support.rs           quote-aware traversal, ASDF designators and plists
└── <rule>/
    ├── rule.rs          META, RULE, the head filter: what the registry registers
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs`'s quote machinery is a deliberate **copy** of
`feature/lint-condition-system`'s, not a dependency on it: two packages of lint
rules should not couple, and the semantics are the part worth sharing. Two
independent counters, because `'` and `` ` `` are not the same thing — a comma
inside `'(…)` is a comma *character* in literal data, so a single depth counter
gets `'(a ,X)` wrong, and a node one level inside a quote is still data, so a
node-local `reader_prefixes` check is not enough either. All five shapes are
pinned by tests in `support.rs` and again in each rule's own tests.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about `defsystem`, `defpackage` or an ASDF operation method | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the four flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |
| teaching the rules a new ASDF option or component class | `support.rs`, or the rule's own constant table |

| You are… | and it does **not** belong here because… |
| --- | --- |
| resolving `.asd` component paths on disk | that is `core/workspace`'s `parse_asdf`, surfaced by `inspect sources` |
| detecting package name/nickname collisions | that is `inspect package-conflicts` |
| deciding what a package ought to export | that is `inspect api-surface` / `inspect unused-exports` |
| writing a rule about `:around` methods in general | that is `feature/lint-object-system` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
