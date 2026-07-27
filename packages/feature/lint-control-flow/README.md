# paredit-feature-lint-control-flow

Lint rules for sequencing, iteration and non-local exit.

## Responsibilities

Eleven rules about the forms that decide *when* and *whether* code runs, rather
than what it computes.

| Rule | Flags |
| --- | --- |
| `binds-constant` | an iteration or binding form binding a value that never changes |
| `eval-when-situation` | an `eval-when` whose situation list cannot have the intended effect |
| `explicit-nil-return` | a trailing `nil` that the form already returns |
| `handler-case-no-clauses` | a `handler-case` that handles nothing |
| `malformed-iteration-spec` | an iteration spec the loop macro cannot mean |
| `nested-progn` | a `progn` directly inside a `progn` |
| `prog2-to-progn` | a two-argument `prog2`, which is `progn` |
| `redundant-body-progn` | a `progn` wrapping a body that is already implicit |
| `redundant-prog1` | a `prog1` whose value is discarded |
| `redundant-progn` | a `progn` with one form |
| `unwind-protect-no-cleanup` | an `unwind-protect` with an empty cleanup |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No `progn` refactoring.** `flatten_progn`, `redundant_progn` *removal* and
  the `let`/`flet` reshaping live in `feature/binding`. A rule that reports a
  redundant `progn` and a command that removes one are different products —
  which is exactly why §5.2.1's grouping of `*_report` slices into refactoring
  features had to be undone.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms; `progn`, `unwind-protect` and the loop keywords are classified there. |
| `paredit-core-semantics` | The rules that must know whether a binding is read before calling it constant. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated control forms. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about `progn`, `unwind-protect`, `eval-when` or iteration | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the eleven flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |

| You are… | and it does **not** belong here because… |
| --- | --- |
| making a command that *removes* a redundant `progn` | that is `feature/binding`; a rule reports, a refactoring rewrites |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
