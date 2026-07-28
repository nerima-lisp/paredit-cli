# paredit-feature-lint-numeric

Lint rules for equality predicates, arithmetic identities and iteration steps.

## Responsibilities

Twenty-one rules about numbers and the predicates that compare them — the
corner of Common Lisp where the right answer depends on which of five equality
operators you meant.

| Rule | Flags |
| --- | --- |
| `eq-char-comparison` | `eq` on characters, where only `eql` is defined |
| `eq-number-comparison` | `eq` on numbers, same problem |
| `eql-list-comparison` | `eql` on lists, which compares identity not contents |
| `eql-string-comparison` | `eql` on strings, same problem |
| `equality-arity` | an equality predicate given the wrong number of arguments |
| `single-arg-comparison` | a comparison with one argument, always true |
| `self-comparison` | a value compared with itself |
| `nil-comparison` / `t-comparison` | comparing against `nil` or `t` where a predicate reads better |
| `sign-comparison` | a sign test written as a comparison |
| `verbose-negation` | a negated comparison with a direct opposite |
| `identity-arithmetic` | arithmetic with an identity element: `(+ x 0)`, `(* x 1)` |
| `single-operand-arithmetic` | an arithmetic form with one operand |
| `one-step-arithmetic` | `(+ x 1)` where `1+` is idiomatic |
| `redundant-divisor` / `zero-divisor` | division by one, and by zero |
| `explicit-step-delta` / `negated-step-delta` / `step-zero` | iteration steps that are redundant, inverted, or never advance |
| `literal-place` | a literal used where a settable place is required |
| `modify-macro-arity` | a modify macro given the wrong arity |

That list is the package's real specification. §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary — §4.2's cycle-avoidance.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No numeric refactoring.** These rules report; nothing here rewrites.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms; which operator a head names is classified there. |
| `paredit-core-semantics` | The rules that must know a name's value before judging an identity. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated arithmetic and comparison forms. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
```

`eq_char_comparison`, `eq_number_comparison` and `zero_divisor` keep their
registry-driven tests in the root as `domain::lint::rule_registry_tests`: those
tests call `collect_lint_findings`, which needs the registry this package
deliberately does not have.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about equality, arithmetic or iteration steps | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the twenty-one flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule about sequences, strings or control flow | it belongs to its own themed package |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
