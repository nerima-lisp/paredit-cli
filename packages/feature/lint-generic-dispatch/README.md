# paredit-feature-lint-generic-dispatch

Lint rules for the **CLOS generic-function dispatch protocol**: the rules of
CLHS chapter 7 that a program can break while compiling cleanly, and then fail
at load time or on one argument combination at run time.

`paredit-feature-lint-object-system` reads CLOS *definitions* — slot options,
duplicate method signatures, a generic with no methods, an `:around` that never
chains. This package reads the *protocol*: whether a method may be added to its
generic function at all, whether the initialization protocol still runs when a
method overrides part of it, and whether two slot options contradict each other
about what a slot is for.

## The rules

| rule | category | severity | fixability | heads | dialect scope |
|---|---|---|---|---|---|
| `defgeneric-method-option-incongruent` | `ObjectSystem` | `Error` | `ReportOnly` | `defgeneric` | Common Lisp |
| `initialization-primary-without-call-next-method` | `ObjectSystem` | `Error` | `ReportOnly` | `defmethod` | Common Lisp |
| `class-allocated-slot-with-initarg` | `ObjectSystem` | `Warning` | `ReportOnly` | `defclass` | Common Lisp |

Every rule declares `HeadFilter::Heads`, so a file with none of those three
heads reaches no `check` body in this package at all.

Each rule's module documentation carries the CLHS section it implements and the
**exact SBCL 2.6.0 expression** that was run to settle it, with the output.

## Cost

**Every rule here is local to the form the dispatcher hands it.** None reads
another node in the file. That is a result rather than a starting assumption:

- `defgeneric-method-option-incongruent` was first written to correlate a
  `defgeneric` with every `defmethod` in the file naming it. It was correct and
  passed the corpus audit clean, and it was dropped on measurement — 4.37 s at
  2000 protocols, an 8x doubling ratio of 97, and *slower than the shipped
  `select_path` scan it was written to beat*. Nothing bounded it:
  `RuleContext::scratch_cache` is a single slot already owned by
  `paredit-feature-lint-repl-debug`, and `RuleContext::binding_table` models
  lexical scopes rather than definition names. Two shipped rules in
  `lint-object-system` already have that shape and are on record as dominating
  lint time at ~480 definitions; a third was not worth adding.
- `is_unevaluated_at` was first written to descend from `SyntaxTree::root_view`,
  which materializes the whole document — once per *finding*. That alone was
  4.49 s on a 2000-form file where every form reports. It now answers from the
  node's own reader prefixes plus a binary search over
  `SyntaxTree::root_child_span` (allocation-free; `Path::root_child`
  heap-allocates), and materializes at most one top-level form, and only for a
  nested match.

Measured, all three fixtures, at load average 72 (`cost_tests`, 8x range
250→2000, linear is ~8):

| fixture | rule | 8x ratio | ns/invocation at n=2000 |
|---|---|---|---|
| clean | `defgeneric-method-option-incongruent` | 9 | 1056 |
| clean | `initialization-primary-without-call-next-method` | 15 | 75 |
| clean | `class-allocated-slot-with-initarg` | 11 | 329 |
| clean | `cost-control-noop` | 9 | 20 |
| clean | `cost-control-select-path-scan` *(the shipped pairing shape)* | **72** | **374361** |
| incongr *(every form reports)* | `defgeneric-method-option-incongruent` | 16 | 1207 |
| incongr | `cost-control-noop` | 8 | 22 |
| incongr | `cost-control-select-path-scan` | **73** | **60443** |

Every rule here tracks the no-op control. Nothing here tracks the scan. That is
the property `cost_tests` guards.

## Registration

This package is deliberately **not** wired into the root `REGISTRY`. A separate
pass does that.
