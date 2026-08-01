# paredit-feature-lint-object-system

Lint rules for CLOS classes, generic functions and methods.

## Responsibilities

Eight rules about the Common Lisp Object System: what a `defclass` says about
its slots, and whether the `defgeneric`/`defmethod` forms around it dispatch the
way their author meant.

| Rule | Flags |
| --- | --- |
| `around-method-missing-call-next-method` | an `:around` method whose body never calls `call-next-method` |
| `defclass-required-slot-no-initform-or-initarg` | a slot with no `:initform` and no `:initarg` that a method in the file reads |
| `defclass-slot-shadowing` | a subclass slot that silently shadows a same-file superclass slot |
| `duplicate-defmethod-signature` | two `defmethod`s with the same name, qualifiers and specializers |
| `generic-function-no-methods` | a `defgeneric` no `defmethod` in the file ever specializes |
| `method-qualifier-typo` | a `defmethod` qualifier outside `:before`/`:after`/`:around` |
| `print-object-without-print-unreadable-object` | a `print-object` method that writes to the stream directly |
| `slot-value-bypasses-accessor` | `(slot-value o 'x)` where the file declares an accessor for `x` |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary. A registry here would be the cycle §4.2
  exists to prevent.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **Not the slot-option or lambda-list rules.** `defclass-slot-option`
  (is this slot option spelled like a real one?) and
  `method-lambda-list-mismatch` (does this method's lambda list agree with its
  generic's?) live in `feature/lint-convention` and are not restated here.
- **No CLOS reports.** `inspect class-hierarchy`, `inspect generic-dispatch`
  and `inspect duplicate-methods` are `feature/lisp-analysis`'s. A report that
  describes a whole file's class graph and a rule that flags one form are
  different products.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms; `defclass`, `defgeneric` and `defmethod` are read there. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself, plus its unit tests
├── usecase.rs   the report's gate
└── cli/         the standalone `inspect <rule>` subcommand
```

`src/support.rs` is what the eight share: the quote-aware descent that decides
whether a matched form is code or unevaluated data, the cheap top-level scan
that correlates separate definitions, and the `defmethod` signature reader.

## Cost

Every rule declares `HeadFilter::Heads`, so the engine's head index means a
file with no `defclass`, `defgeneric`, `defmethod` or `slot-value` never enters
this package at all. Nothing here is allocated per visited node, and no rule
asks `RuleContext` for a semantic table — the correlation these rules need is
between *definitions*, which `support` reads straight off the tree.

## Dialect

Common Lisp only. CLOS is a Common Lisp standard; `defclass` in Clojure or
Scheme means whatever a local macro says it does, so `dialect_scope()` is left
at its `COMMON_LISP_ONLY` default.
