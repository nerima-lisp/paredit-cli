# paredit-feature-lint-form-shape

Lint rules for form shape: assignment, quoting, lambda, keywords, arity and defaults.

## Responsibilities

Thirty-eight rules about *how a form is written* rather than what subject it is
about. This is the group that takes what the other five themes do not, which is
why it is the largest.

| Group | Rules |
| --- | --- |
| assignment | `self-assignment`, `setf-arity`, `setq-non-variable`, `duplicate-setf-places`, `manual-incf`, `manual-push`, `manual-pushnew` |
| quoting and function values | `redundant-quote`, `sharp-quoted-lambda`, `funcall-lambda`, `redundant-funcall`, `redundant-apply`, `redundant-identity` |
| type declarations | `redundant-the`, `the-arity`, `typep-predicate`, `coerce-to-t` |
| lambda lists and keywords | `duplicate-parameters`, `duplicate-keyword`, `duplicate-lambda-list-keyword`, `lambda-list-keyword-order` |
| defaulted arguments | `make-array-default-keyword`, `make-hash-table-test`, `make-list-default-element`, `getf-default-nil`, `gethash-default`, `butlast-default-count`, `parse-integer-default-radix` |
| binding shape | `duplicate-let-bindings`, `malformed-let-binding`, `empty-let`, `redundant-let-star`, `single-value-bind` |
| multiple values | `multiple-value-list-of-values`, `values-list-of-list` |
| nesting | `nested-cxr`, `nested-char-case` |
| packages | `defpackage-quoted` |

That table is the package's real specification. §5.2.2 splits by subject
matter, so naming the rules is the only way to say why one belongs here.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary — §4.2's cycle-avoidance.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No refactoring.** `feature/binding` reshapes `let` forms and
  `feature/function-parameter` edits lambda lists; these rules only report.
  A rule that flags a duplicate parameter and a command that removes one are
  different products — which is precisely why §5.2.1's grouping of these
  `*_report` slices into the refactoring features had to be undone.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms; lambda-list keywords, reader prefixes and operator shapes are classified there. |
| `paredit-core-semantics` | The rules that must know whether a place is a variable, or a value a literal. |
| `paredit-core-workspace` | Each rule's own subcommand scans a workspace. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `anyhow`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated forms. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
```

Three rules name their report module in the singular where the rule is plural
(`duplicate_let_bindings` → `duplicate_let_binding_report`, and likewise
`duplicate_parameters`, `duplicate_setf_places`).

`parse_integer_default_radix` and `redundant_the` keep their registry-driven
tests in the root as `domain::lint::rule_registry_tests`.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about assignment, quoting, arity or a defaulted argument | it is a new slice here, plus one line in the root's REGISTRY |
| changing how a defaulted keyword argument is recognised | seven rules here share that shape |
| changing what one of the thirty-eight flags | that rule's `domain.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| adding a command that removes a duplicate parameter or reshapes a `let` | those are `feature/function-parameter` and `feature/binding` |
| adding a rule that clearly belongs to another theme | put it there; this package is the remainder, not the default |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
