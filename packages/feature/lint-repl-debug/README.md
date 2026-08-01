# paredit-feature-lint-repl-debug

Lint rules for leftover REPL-debugging artifacts left in committed source.

## Responsibilities

Eight rules about the traces an interactive debugging session leaves behind
when a form is pasted into a REPL, evaluated to see what happens, and never
cleaned back out before the commit.

| Rule | Flags |
| --- | --- |
| `leftover-print-debug` | a bare debug-print call (`princ`/`print`/`prin1`/`pprint`/`message`/`println`/`prn`/`display`/`displayln`/`pp`, per dialect) |
| `leftover-trace-call` | `trace`/`untrace` used as a statement |
| `leftover-break-call` | a Common Lisp `(break ...)` |
| `leftover-inspect-call` | a Common Lisp `(inspect x)` / `(describe x)` |
| `leftover-time-benchmark-call` | a Common Lisp `(time form)` wrapper |
| `leftover-step-call` | a Common Lisp `(step form)` wrapper |
| `commented-repl-transcript` | a comment block shaped like a pasted REPL session |
| `leftover-format-debug-marker` | a `(format t ...)` whose control string carries a `DEBUG`/`DBG` marker |

That list is the package's real specification: every rule flags code that is
locally well-formed and does exactly what it says, and whose defect is only
that a human forgot to remove it before committing.

### Shared removal-safety analysis

`leftover-print-debug`, `leftover-trace-call`, `leftover-break-call`,
`leftover-inspect-call` and `leftover-format-debug-marker` all remove a whole
call form when they can prove it safe. "Safe" is the same question for all
five — is this form a bare top-level form, or a non-last form of its
enclosing implicit-progn/`cond`-clause body? — answered once in
[`crate::support`] instead of five times. `leftover-time-benchmark-call` and
`leftover-step-call` do not need it: unwrapping `(time form)`/`(step form)` to
`form` is value-preserving in every position, per the CLHS (see each rule's
own module doc for the citation).

`commented-repl-transcript` deliberately implements no fix. Comments live
outside the node tree this project's rewrites walk (see
`docs/src/reference/architecture.md`), and a prior incident in this exact
codebase was a write command silently dropping every comment in a file for
exactly that reason. Proving a comment-block removal safe is out of this
package's scope.

### What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`, `RuleFix`. |
| `paredit-core-syntax` | Rules match on parsed forms and reader prefixes. |
| `paredit-core-cli` | Input reading, shared argument types, `safe_text!`. |
| `clap`, `serde_json`, `thiserror` | Arguments, JSON output, fallible paths. |
| `proptest` (dev) | Properties over generated forms. |

## Layout

One rule, one directory — the four files a rule is made of:

```text
src/<rule>/
├── rule.rs      META, RULE, the head filter: what the registry registers
├── domain.rs    the detection itself
├── usecase.rs
└── cli/         the `inspect <rule>` subcommand
src/support.rs   shared removal-safety walk used by the five removal rules
```

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about a REPL-debugging leftover | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the eight flags, or how it phrases it | that rule's `domain.rs` |
| changing when a removal fix is safe to emit | `src/support.rs`, shared by five of the eight rules |

| You are… | and it does **not** belong here because… |
| --- | --- |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
