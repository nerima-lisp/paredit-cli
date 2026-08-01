# paredit-feature-lint-iteration-flow

Lint rules for Common Lisp iteration constructs.

## Responsibilities

Six rules about the standard iteration macros — `loop` and the
`dolist`/`dotimes` pair — and specifically about the parts of them that a
tree walker cannot see without reading the macro's own grammar.

| Rule | Flags |
| --- | --- |
| `loop-clause-order-violation` | a `loop` variable clause after a main clause, or `named` other than first |
| `loop-into-accumulator-kind-conflict` | one `into` variable accumulated as both a list and a number |
| `loop-unreachable-finally-clause` | epilogue forms after a `finally` clause that returns |
| `dotimes-bound-mutation-has-no-effect` | assigning the `dotimes` count variable inside the body |
| `loop-for-across-statically-known-list` | `for … across` over a value that is provably a list |
| `dolist-result-form-references-loop-variable` | a `dolist` result form reading the loop variable, which is `nil` there |

There is deliberately no rule about a `do` step form reading a variable bound
later in the same varlist. Reading a later variable's *previous* value is the
whole reason `do` exists as distinct from `do*`, and every parallel swap,
rotate, and Fibonacci `do` does it on purpose.

## Scope

Common Lisp only. Every rule encodes CLHS operator semantics, so the default
[`RuleDialectScope::COMMON_LISP_ONLY`] applies.

## What this package deliberately does not do

`loop` has a grammar of its own rather than an S-expression shape, and this
package does **not** implement it in full. Each rule declares in its own module
doc exactly which clause shapes it recognises and which it bails out of. The
shared reader in [`loop_syntax`] is a conservative tokenizer: whenever it
cannot be sure what a top-level token means, it reports nothing rather than
guessing, because a false positive here becomes a warning on correct,
idiomatic user code.

The complementary non-lint `inspect loop` report lives in
`paredit-feature-lisp-analysis`'s `loop_report`. Its `conflicting-accumulation`
finding covers the *implicit* result — two accumulation verbs with **no**
`into` — and explicitly drops any verb that names an `into` target.
`loop-into-accumulator-kind-conflict` here covers exactly the case that report
drops.

[`RuleDialectScope::COMMON_LISP_ONLY`]: paredit_core_lint_engine::policy::RuleDialectScope
