# paredit-feature-lint-scheme-idiom

Lint rules whose subject is Scheme or Racket itself.

The shipped catalogue is overwhelmingly `COMMON_LISP_ONLY`, because almost
every rule in it encodes a CLHS operator's semantics. The rules here encode
R7RS and Racket semantics instead, and each one names the section it is
derived from in its own module documentation. A rule is in this package only
when the standard says something a Common Lisp rule cannot: `set!` is not
`setq`, R7RS `eqv?` is not `eql`, and Racket's `#lang` module language is not
a `defpackage`.

Every rule anchors on `HeadFilter::Heads`. `WholeTree` runs once per file for
every file regardless of dialect, and this package is scoped to two dialects
out of eleven — paying a whole-tree walk on every Common Lisp file to answer a
Scheme question is exactly the cost the head index exists to avoid.

## The rules

| Rule | Heads | Dialects |
| --- | --- | --- |
| `scheme-begin-single-form` | `begin` | Scheme, Racket |
| `scheme-let-star-independent-bindings` | `let*` | Scheme, Racket |
| `scheme-memq-assq-literal-key` | `memq`, `assq` | Scheme |
| `scheme-named-let-never-recurs` | `let` | Scheme, Racket |

There is no `SCHEME_ONLY` constant on `RuleDialectScope`, so each rule declares
a file-local `DIALECTS` in its own `domain` and its `rule.rs` passes that same
constant to both `RuleDialectScope::new` and the report's dialect gate. The two
therefore cannot drift apart.

## Dialect scope is a claim about the standard, not a default

`scheme-memq-assq-literal-key` is Scheme-only on purpose. Racket's `memq` is
`eq?`-based like everyone else's, but Racket *specifies* the two cases R7RS 6.4
leaves open — fixnums compare `eq?` by guarantee, and characters have been
normatively `eq?` since 9.0.0.10 — so every finding the rule could produce on
Racket would be a complaint about code the language promises will work.

This is not a hypothetical. An earlier draft of this package carried a rule
flagging `eq?` on a number or character literal, on the same R7RS 6.1 reasoning.
Measured over 3 MB of real Scheme it produced **15 findings against 462
candidate `eq?` forms, and not one was a defect**: eleven compared characters
and four compared the fixnums `0` and `1`, every one of them inside the range
Racket guarantees. The rule was deleted rather than narrowed, because after
removing the guaranteed cases nothing syntactically detectable remained — a
rule cannot tell a fixnum from a bignum at a computed call site. `memq`/`assq`
replaced it because R7RS 6.4 states the verdict outright, with worked examples,
and because the repair (`memq`→`memv`, `assq`→`assv`) is mechanical and cannot
break a working search.

## False positives found by corpus audit

Each of these was found by running the rules over the Guile 3.0.11 standard
library, not by review, and each has a regression test reduced from the file
that produced it:

- **A named `let` in a `syntax-rules` template is not a dead loop.** Both the
  loop name and the body are pattern variables the macro's caller supplies, so
  the recursive call is nowhere in the template's own text.
  (`ice-9/match.upstream.scm`, `srfi/srfi-71.scm`.)
- **A `(begin ,form)` at quasiquote depth zero is a matcher clause pattern,
  not a form.** An unquote with no quasiquote above it is not evaluable Scheme
  at all. (`language/ecmascript/compile-tree-il.scm`.)

Both suppressions are asymmetric on purpose: they can only lose findings, never
invent them, which is the direction a rule of this kind has to err in. Each is
paired with a positive control asserting the ordinary shape still fires, so the
exemption cannot silently widen into "never report anything".
