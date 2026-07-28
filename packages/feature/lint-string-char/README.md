# paredit-feature-lint-string-char

Lint rules for characters, strings and format directives.

## Responsibilities

Eight rules, grouped because they all reason about the same corner of Common
Lisp: the boundary between characters, strings, and how text gets printed.

| Rule | Flags |
| --- | --- |
| `char-case-fold` | a case-insensitive character comparison written as a case-sensitive one |
| `char-op-string` | a character operator (`char=`, `char<`…) given a string literal — a type error |
| `code-char-char-code` | `(code-char (char-code c))`, which is just `c` |
| `format-missing-destination` | `format` called without its destination argument |
| `format-newline` | a literal newline where `~%` is meant |
| `format-to-string` | `(format nil …)` where a direct string operation is clearer |
| `nested-string-case` | a string case fold applied twice |
| `string-case-fold` | a case-insensitive string comparison written as a case-sensitive one |

That list is the package's real specification. §5.2.2's split is by *subject
matter*, and the only way to say why a rule belongs here rather than in
`lint-sequence` is to name them.

### What this package does not own

- **No registry.** The rule set is enumerated by `REGISTRY` in the root crate,
  which reaches each rule's `META` and `RULE` across this boundary. A registry
  here would be the cycle §4.2 exists to prevent: it would name every rule, in
  a crate every rule depends on.
- **No engine.** The single pass, the head index and the rule trait are
  `paredit-core-lint-engine`'s.
- **No lint command.** `inspect lint` is the root's, because running the whole
  rule set means having the registry.

Each rule *does* own its `inspect <rule>` subcommand, which reports that rule
alone and needs no registry.

## Why these eight, and not others

§5.2.2 calls its six-way split "the one part of this document based on
judgement rather than measurement", and asks for it to be re-verified against
the head symbols each rule actually filters on. Doing that — clustering all 134
rules by their `HEADS` arrays, over 204 distinct head symbols — gives:

| package | measured | §5.2.2 estimate |
| --- | ---: | ---: |
| lint-form-shape | 38 | 25 |
| lint-conditional | 33 | 25 |
| lint-sequence | 23 | 25 |
| lint-numeric | 21 | 20 |
| lint-control-flow | 11 | 18 |
| **lint-string-char** | **8** | 13 |

The axis holds; only the sizes shift. This is the smallest of the six, which is
why it went first: it is the cheapest place to find out whether the four-file
rule shape survives extraction.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext` — the contract every rule implements. |
| `paredit-core-syntax` | Rules match on parsed forms; the character and string literal shapes come from the reader. |
| `paredit-core-semantics` | The few rules that need to know a name is bound before judging it. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, the `safe_text!` rendering guard. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated string and character forms. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
```

`char_op_string`'s tests are the exception: they call `collect_lint_findings`
and filter for their own rule, which needs the registry, so they live in the
root as `domain::lint::rule_registry_tests`. Seven other rules test themselves
that way.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about characters, strings or `format` | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the eight flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a rule about sequences, arithmetic or control flow | it belongs to its own themed package |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
