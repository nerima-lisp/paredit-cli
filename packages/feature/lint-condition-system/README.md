# paredit-feature-lint-condition-system

Lint rules for the Common Lisp condition system.

## Responsibilities

Seven rules about `define-condition`, `restart-case`, `handler-bind`, and the
signalling operators — the half of the condition system that decides whether an
error is *reported usefully*, *caught at all*, or *silently dropped*.

| Rule | Flags |
| --- | --- |
| `cerror-missing-continue-format` | a `cerror` whose continue-format-control is missing or `nil` |
| `define-condition-empty-superclass-list` | a `define-condition` with `()` supertypes, which defaults to `condition`, not `error` |
| `define-condition-missing-report-for-error-type` | an `error` subtype with no `:report` anywhere in its same-file ancestry |
| `handler-bind-handler-returns-bare-value` | a `handler-bind` handler ending in a value `handler-bind` throws away |
| `ignore-errors-wraps-non-error-signal` | an `ignore-errors` around a `signal` of a non-`error` condition it cannot catch |
| `restart-case-clause-without-report` | a `restart-case` clause with no `:report`, other than the five CLHS-standard restart names (`continue`, `abort`, `use-value`, `store-value`, `muffle-warning`), whose bare name is the documented interface |
| `signal-on-error-condition-returns-silently` | a `signal` of an `error` subtype, which returns `nil` when unhandled |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

Every rule is `Fixability::ReportOnly`. What a missing `:report` *should say*,
which supertype was meant, and whether a handler meant to decline are all
questions a rewrite cannot answer, so none of them ships a fix.

Every rule is also `Severity::Warning`, deliberately and not by default. The two
`conditions`-category rules that are `Severity::Error` —
`handler-case-swallows-error` and `unreachable-handler-clause` — flag code whose
behaviour is settled by the form itself: a clause that cannot run, an error that
provably goes nowhere. Each of the seven here flags an inference about *intent*
that has a legitimate reading — `(signal 'my-error)` is also how a condition
protocol offers handlers a choice and then continues, an undocumented restart may
be one only other code invokes, an `ignore-errors` may be guarding a call for a
reason unrelated to the `signal` inside it. `Warning` is the honest level for a
finding a competent author may read and deliberately keep.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No handler-*case* rules.** `handler-case-swallows-error` and
  `unreachable-handler-clause` are `feature/lint-safety`'s;
  `handler-case-no-clauses` is `feature/lint-control-flow`'s. This package is
  the definition and signalling side, plus `handler-bind`.
- **No format-string arity.** A rule named `error-format-string-argument-mismatch`
  was asked for and is deliberately **not implemented here**: `~` directives
  versus supplied arguments — including for `error`, `warn` and `cerror`, which
  `feature/lisp-analysis`'s `format_directive_report` lists by name with their
  control-string positions — is already that report's subject. A second counter
  would be a second answer.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms and on Common Lisp operator spelling. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── support.rs           quote-aware traversal and the same-file condition hierarchy
└── <rule>/
    ├── rule.rs          META, RULE, the head filter: what the registry registers
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs` exists because three rules must correlate a call site with a
`define-condition` elsewhere in the same file, and all seven must agree on what
counts as unevaluated data. Both are built *after* a candidate is found, never
per visited node.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about `define-condition`, `restart-case`, `handler-bind` or a signalling operator | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the seven flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |
| teaching the suite a new standard condition subtype | `support.rs`'s hierarchy table |

| You are… | and it does **not** belong here because… |
| --- | --- |
| writing a rule about a `handler-case` clause | that is `feature/lint-safety` / `feature/lint-control-flow` |
| counting `~` directives against arguments, e.g. `error-format-string-argument-mismatch` | that is `feature/lisp-analysis`'s `format_directive_report` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
