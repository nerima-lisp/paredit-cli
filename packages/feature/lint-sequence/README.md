# paredit-feature-lint-sequence

Lint rules for list and sequence operations and their default arguments.

## Responsibilities

Twenty-three rules about the sequence library — the largest source of Common
Lisp forms that are correct but say more than they need to.

| Rule | Flags |
| --- | --- |
| `accessor-arity` | a `c[ad]+r` accessor given the wrong arity |
| `append-list-to-cons` | `(append (list x) y)`, which is `(cons x y)` |
| `append-nil` | `append` with a `nil` argument |
| `car-nthcdr` | `(car (nthcdr n l))`, which is `(nth n l)` |
| `car-reverse` | `(car (reverse l))`, which is `(car (last l))` |
| `cons-to-list` | `(cons x nil)`, which is `(list x)` |
| `double-reverse` | `reverse` of a `reverse` |
| `destructive-literal` | a destructive operation applied to a literal |
| `eql-search-literal` | a search predicate defaulted to `eql` on a literal |
| `list-star-nil` / `list-star-to-cons` | `list*` degenerating to `list` or `cons` |
| `nth-constant-index` / `nthcdr-small-index` / `nthcdr-zero` | index forms with a shorter accessor |
| `subseq-zero` | `subseq` from zero, which is a copy |
| `last-default-count` | `last` with its default count spelled out |
| `redundant-count-nil`, `redundant-end-nil`, `redundant-eql-test`, `redundant-from-end-nil`, `redundant-identity-key`, `redundant-start-zero` | keyword arguments passed at their default |
| `single-operand-list-op` | a list operation with one operand |

Six of these are the same observation applied to six different keyword
defaults, which is why they cluster: fixing how the tool recognises a defaulted
keyword touches all of them at once.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary — §4.2's cycle-avoidance.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No sequence refactoring.** These rules report; nothing here rewrites.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms; which sequence operator a head names, and its lambda list, are classified there. |
| `paredit-core-semantics` | The rules that must know a value is a literal before calling a destructive call unsafe. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated sequence calls. |

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
| adding a rule about a sequence operator or a defaulted keyword | it is a new slice here, plus one line in the root's REGISTRY |
| changing how a defaulted keyword argument is recognised | it likely touches all six `redundant-*` rules together |
| changing what one of the twenty-three flags | that rule's `domain.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule about arithmetic, strings or control flow | it belongs to its own themed package |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
