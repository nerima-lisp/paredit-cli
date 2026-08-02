# paredit-feature-lint-contract-annotation

Lint rules about *declared* contracts: a type annotation, a condition vector, or
a runtime type assertion that does not say what its author meant.

Two rules across two dialects. Both are `Fixability::ReportOnly` — in each case
two different repairs are available and only the author knows which was
intended — and both are `HeadFilter::Heads`.

| rule | category | severity | heads | dialect |
| --- | --- | --- | --- | --- |
| `typed-racket-arity-mismatch` | `Arity` | `Warning` | `define` | Racket |
| `clojure-pre-post-vacuous` | `Suspicious` | `Warning` | `defn`, `defn-` | Clojure |

## Four rules that were proposed and dropped

Each was dropped because the language premise behind it turned out to be false.
They are recorded here so nobody re-proposes them.

Two of the four — `check-type-redundant-with-declare` and
`clojure-pre-referencing-percent` — were **written, reviewed and pushed** before
the refutation landed, and were removed from the branch before merge. If you are
reading this because you found one of them in the git history and wondered where
it went, the answer is below.

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

**`check-type-redundant-with-declare`** — "a `(check-type x integer)` restating
the type an adjacent `(declare (type integer x))` already promised for that
variable".

The premise is **inverted**. It reads the pair as one idea spelled twice, with
the `check-type` as the redundant half. It is the `declare` that guarantees
nothing.

CLHS **3.3.1, "Minimal Declaration Processing Requirements"**, verbatim:

> In general, an implementation is free to ignore declaration specifiers except
> for the `declaration`, `notinline`, `safety`, and `special` declaration
> specifiers.

`type` is not in that exempt list, so a conforming implementation may discard
`(declare (type integer x))` outright. `d_type.htm` completes the picture: the
consequences of violating a type declaration are **undefined**, not signalled.
So of the two forms, `check-type` is the only portable guarantee in the pair —
the opposite of what the rule assumed.

The Google Common Lisp Style Guide recommends precisely the code this rule
flagged: *"You should prefer to use CHECK-TYPE over (DECLARE (TYPE ...)) for the
inputs of functions"*, noting that `(declare (type ...))` may generate no check
at all depending on optimization policy.

Three further points, any one of which is on its own disqualifying:

1. **SBCL's behaviour is not a defence.** SBCL does check type declarations at
   the default policy, but that is an SBCL-specific deviation *toward* checking,
   and it is policy-dependent. A `(declaim (optimize (safety 0)))` in another
   file of the same system turns those checks off, and a syntactic linter
   examining one file cannot see it.
2. **The two forms are not interchangeable.** `check-type` establishes a
   `store-value` restart, so a caller can correct the value and continue.
   `declare` has no equivalent. Deleting the `check-type` on the rule's advice
   removes a recovery path, not a duplicate line.
3. **The direction of the "fix" was never determinable.** The rule was
   `ReportOnly` for that reason, which should itself have been the warning sign.

**`clojure-pre-referencing-percent`** — "a `defn` `:pre` condition naming `%`,
which `clojure.core`'s `fn` binds only inside `:post`".

Verified against `clojure/src/clj/clojure/core.clj` at master. With `*assert*`
true — the default — `{:pre [(pos? %)]}` is a **compile-time
`CompilerException`**: `Unable to resolve symbol: % in this context`, raised when
the namespace loads. Every user already receives this error, unavoidably, before
any lint run. The rule re-reports a compile error, which is the same objection
that sank `racket-contract-out-stale-name` above.

Its only residual value would be under a production profile with `*assert*`
false — where the conditions are compiled away and are therefore not running
anyway.

The rule's framing was also wrong on the facts. Only the bare symbol `%` is
injected: `~'%` in the `fn` macro at core.clj:4716-4726. **`%1` is not bound in
`:post` either.** So "referencing `%`/`%1` in `:pre`" carries the implication
that `%1` is fine in `:post`, and it is not — a user who moved a `%1` condition
from `:pre` to `:post` on this rule's advice would get the identical compile
error at the new location.

Removing it also removed `support.rs`'s `is_function_literal` and
`is_percent_parameter`, which existed only to keep it off a `%` that was a
`#(…)` literal's own parameter.

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

Stating the scope explicitly matters more here than it looks: Common Lisp is the
`RuleDialectScope` **trait default**. Since `check-type-redundant-with-declare`
was dropped, no rule in this package models Common Lisp at all, so a rule that
silently lost its `dialect_scope` override would not merely widen — it would
start walking every `.lisp` file in a repository. `no_rule_runs_for_common_lisp`
in `src/lib.rs` asserts the zero directly.

## Cost

Both rules are `Heads`, and both call into `support` only *after* the head has
matched — the `clean/forms/*` benchmarks lint zero-finding files, so the
per-file cost of a rule that matches nothing is exactly what they measure. Both
are moreover skipped before the walk begins for any Common Lisp file, because
the dispatcher resolves the dialect scope first.

`src/cost_tests.rs` drives the real dispatcher with `PassOptions { measure:
true }` and pins the shape rather than the constant: microseconds and invocation
counts per rule at 1000/2000/4000/8000 definitions, against a no-op control rule
declaring the same heads, asserting that an 8× larger file costs under 20× more.
Linear is 8×; the quadratic shape that has twice shipped in this codebase is
about 64×.

## False negatives are deliberate

Each rule's module documentation lists what it declines to look at and why.
Across the package the direction is consistent: prefer a missed finding to a
wrong one. The `engine_pass_tests` module runs both rules over hand-written
*idiomatic correct* files in both dialects and requires silence, paired with two
"dangerous twin" files — each the correct file with one thing made wrong — so
that a sweep which silently stopped detecting anything cannot pass.

`typed-racket-arity-mismatch` is where this discipline earns its keep, because
naive positional counting is wrong on most of Typed Racket's arrow grammar. The
rule counts only the plain all-positional prefix arrow over plain positional
parameters and declines everything else. The shapes it must decline, each with a
test in `domain.rs`:

| shape | why counting breaks |
| --- | --- |
| `(Number -> Number)` infix | naive count is 0 against a real arity of 1 — would fire on *every* correct infix annotation |
| `(-> Any Boolean : String)` proposition | the `:` ends the `dom` sequence; naive count is 3 against a real arity of 1 |
| `#:kw Type` mandatory keyword | two `dom` elements, one parameter |
| `[#:kw Type]` optional keyword | arity becomes a range |
| `Type *` rest | unbounded arity |
| `Type ooo bound` ellipsis | three extra elements, unbounded arity |
| `->*`, `case->`, `All`/`∀` | a different shape entirely; declined by construction, since the head of the type is not an arrow |
| `(define ((f x) y) …)` curried | the header's head is a list, not a name |
| `(define f (lambda …))` | names no parameter list of its own |
| `(define #:forall (A) (f x) …)` | TR's `maybe-tvars` mean the header is not always at index 1 |
| `. rest`, `#:kw`, `[y 5]` parameters | rest, keyword and optional parameters |

The proposition case is the one that shipped broken: the rule as first written
counted `(-> Any Boolean : String)` as three arguments and reported a mismatch
against the one-parameter `define` below it — a false positive on an example
from the Typed Racket reference itself. `declines_an_arrow_carrying_a_proposition_or_object`
and `declines_the_references_own_keyword_example` pin it.
