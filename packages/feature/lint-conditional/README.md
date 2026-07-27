# paredit-feature-lint-conditional

Lint rules for conditionals, case forms and boolean operators.

## Responsibilities

Thirty-three rules — the largest themed group — about the forms that branch:
`if`, `when`, `unless`, `cond`, `case`, `typecase`, and the boolean operators
that feed them.

| Group | Rules |
| --- | --- |
| `if` shape | `if-arity`, `one-armed-if`, `if-not`, `if-to-or`, `if-to-unless`, `negated-if`, `redundant-if-nil`, `identical-if-branches`, `constant-if-test` |
| `when` / `unless` | `constant-when-test`, `negated-when-unless`, `nested-when`, `nested-unless` |
| `cond` | `cond-t-clause`, `single-clause-cond`, `malformed-cond-clause`, `duplicate-cond-tests`, `unreachable-cond-clause` |
| `case` / `typecase` | `case-nil-key`, `typecase-nil-key`, `quoted-case-key`, `duplicate-case-keys`, `malformed-case-clause`, `unreachable-case-clause`, `exhaustive-case-otherwise` |
| boolean operators | `de-morgan`, `nested-boolean`, `dead-boolean-operand`, `duplicate-boolean-operands`, `redundant-boolean-identity`, `single-operand-boolean`, `negated-comparison` |
| shared | `empty-body` |

That table is the package's real specification. §5.2.2 splits by subject
matter, so naming the rules is the only way to say why one belongs here.

Three of these — unreachability in `cond` and `case`, and duplicate tests —
depend on structural expression equality rather than on syntax alone, which is
why `core/syntax` carries `expression_equality` for them to share.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary — §4.2's cycle-avoidance.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No conditional refactoring.** `convert-if-to-cond`, `convert-cond-to-if`
  and friends are `feature/form-transform`'s. A rule that reports a one-armed
  `if` and a command that rewrites one are different products.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms, and `expression_equality` is what the unreachability and duplicate-test rules compare with. |
| `paredit-core-semantics` | The rules that must know whether a test is constant in context. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated conditionals. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
```

Four rules name their report module in the singular where the rule is plural
(`duplicate_cond_tests` → `duplicate_cond_test_report`, and likewise
`identical_if_branches`, `duplicate_boolean_operands`, `duplicate_case_keys`).
The slice directory uses the rule's name; the alias only mattered while moving.

`constant_if_test` keeps its registry-driven tests in the root as
`domain::lint::rule_registry_tests`, because they call `collect_lint_findings`.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about a branching form or a boolean operator | it is a new slice here, plus one line in the root's REGISTRY |
| changing how unreachability or duplicate tests are detected | those rules share `expression_equality` from core/syntax |
| changing what one of the thirty-three flags | that rule's `domain.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a command that rewrites a conditional | that is `feature/form-transform` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
