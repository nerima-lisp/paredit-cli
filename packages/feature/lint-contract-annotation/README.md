# paredit-feature-lint-contract-annotation

Lint rules about *declared* contracts: a type annotation, a condition vector, or
a runtime type assertion that does not say what its author meant.

Four rules across three dialects. Every one is `Fixability::ReportOnly` — in
each case two different repairs are available and only the author knows which
was intended — and every one is `HeadFilter::Heads`.

| rule | category | severity | heads | dialect |
| --- | --- | --- | --- | --- |
| `typed-racket-arity-mismatch` | `Arity` | `Warning` | `define` | Racket |
| `clojure-pre-post-vacuous` | `Suspicious` | `Warning` | `defn`, `defn-` | Clojure |
| `clojure-pre-referencing-percent` | `Suspicious` | `Warning` | `defn`, `defn-` | Clojure |
| `check-type-redundant-with-declare` | `Declaration` | `Warning` | `check-type` | Common Lisp |

## Two rules that were proposed and dropped

Both were dropped because the language premise behind them turned out to be
false. They are recorded here so nobody re-proposes them.

**`typed-racket-missing-return-type`** — "a `(: name …)` annotation whose `->`
type has fewer than 2 elements, i.e. no explicit return type".

The premise is wrong. The Typed Racket Reference defines `(-> dom ... rng)` as
"the type of functions from the **(possibly-empty)** sequence `dom ...` to the
`rng` type" — the last type is the *result* and the others are the arguments.
So `(-> Number)` is a nullary function returning `Number`, a complete and
correct annotation. The Typed Racket Guide underlines it by giving
`(case-> (-> Number) (-> Number Number))` as the type of a function with one
optional argument. A rule built on this premise would fire on every correctly
annotated thunk in every Typed Racket file.

The corollary is load-bearing for `typed-racket-arity-mismatch` and is pinned by
`a_one_element_arrow_is_a_nullary_function_returning_that_type`.

**`racket-contract-out-stale-name`** — "a `(provide (contract-out …))` entry
naming a symbol that no `define` in the module provides".

Dropped for two independent reasons, either of which is sufficient.

1. Racket already rejects it. `contract-out` provides each identifier "from the
   enclosing module", and an identifier that is not bound there is an
   expansion-time error, not something that survives to run time. A lint rule
   that restates a compile error earns little.
2. It cannot avoid false positives. An identifier can be bound in a module by
   `define-values`, `struct` (which binds a constructor, a predicate and one
   accessor per field), `define-struct`, `define-syntax`, `match-define`,
   `define-runtime-path`, a `require` that is re-provided — or by **any
   user-written macro that expands into a `define`**, which is ordinary Racket
   and which no syntactic rule can see. That last class alone makes the rule
   unusable.

## The `:` head problem

`typed-racket-arity-mismatch` is about `(: name Type)` forms but is anchored on
`define`. That is not a preference: `NormalizedHead::new(":")` fails to compile,
because `packages/core/lint-engine/src/model/head_filter.rs` rejects any head
containing a colon. No rule can ask the head index to show it `(: …)` forms.

So the rule is shown `define` forms and looks *up* one top-level sibling, which
is where Typed Racket's own documentation puts the annotation. An annotation
written anywhere else is a false negative — the alternative, scanning the file
per `define`, is the quadratic shape this package exists not to have.

## Dialect scope

`RuleDialectScope` has `COMMON_LISP_ONLY`, `EMACS_LISP_ONLY` and `CLOJURE_ONLY`
constants but **no `RACKET_ONLY`**, so `typed-racket-arity-mismatch` constructs
its scope explicitly as `RuleDialectScope::new(&[Dialect::Racket])`.

It is the first built-in rule scoped to Racket *alone*. Two existing rules name
`Dialect::Racket` — `lint-repl-debug`'s `leftover-print-debug` (8 dialects) and
`lint-control-flow`'s `self-recursive-tail-call` (all 10 parsed dialects) — but
both are broad multi-dialect scopes that happen to include Racket, and neither
encodes anything Racket-specific. Nothing before this rule modelled Racket as
its own language.

Each rule's scope is a `pub const SCOPE` in its own `domain` module, read both by
`LintRule::dialect_scope` and by the standalone report's `dialect_modelled`
flag. That is one constant rather than two literal dialect comparisons, so the
engine's view of where a rule runs and the report's claim about what it measured
cannot drift apart.

## Cost

Every rule is `Heads`, and every rule calls into `support` only *after* its head
has matched — the `clean/forms/*` benchmarks lint zero-finding files, so the
per-file cost of a rule that matches nothing is exactly what they measure. The
Racket and Clojure rules are moreover skipped before the walk begins for any
Common Lisp file, because the dispatcher resolves the dialect scope first.

`src/cost_tests.rs` drives the real dispatcher with `PassOptions { measure:
true }` and pins the shape rather than the constant: microseconds and invocation
counts per rule at 1000/2000/4000/8000 definitions, against a no-op control rule
declaring the same heads, asserting that an 8× larger file costs under 20× more.
Linear is 8×; the quadratic shape that has twice shipped in this codebase is
about 64×.

## False negatives are deliberate

Each rule's module documentation lists what it declines to look at and why.
Across the package the direction is consistent: prefer a missed finding to a
wrong one. The `engine_pass_tests` module runs all four rules over hand-written
*idiomatic correct* files in all three dialects and requires silence, paired
with four "dangerous twin" files — each the correct file with one thing made
wrong — so that a sweep which silently stopped detecting anything cannot pass.
