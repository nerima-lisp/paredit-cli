# paredit-feature-lint-racket-depth

Lint rules for Racket's *own* surface syntax.

`paredit-feature-lint-scheme-idiom` was the only other package scoping rules to
`Dialect::Racket`, and all four of its rules are Scheme-shaped — `begin`,
`let*`, named `let`, `memq`. Racket's distinctive forms had no coverage.

Every premise below was **executed** against Racket v9.2 (`racket` and `raco`
from nixpkgs) rather than inferred from documentation, and every rule was swept
over a 4492-file corpus of `racket/racket` and `racket/typed-racket`.

## Rules

| Rule | Category | Severity | Fix | Heads |
| --- | --- | --- | --- | --- |
| `racket-match-unreachable-clause` | `dead-code` | error | report-only | `match`, `match-lambda`, `match-lambda*` |
| `racket-for-comprehension-value-discarded` | `allocation` | warning | report-only | `begin`, `when`, `unless`, `lambda`, `λ`, `define`, `let`, `let*`, `letrec`, `letrec*`, `parameterize` |
| `racket-begin0-single-form` | `suspicious` | warning | **fixable** | `begin0` |
| `racket-case-lambda-single-clause` | `suspicious` | warning | **fixable** | `case-lambda` |
| `racket-parameterize-empty-bindings` | `suspicious` | warning | report-only | `parameterize` |

Every rule sets `dialect_scope()` explicitly from its own `domain::DIALECTS`.
The engine default is `COMMON_LISP_ONLY`, so a rule here that forgot it would
never run on Racket at all while still passing every unit test that calls
`check` directly. `tests/engine_pass.rs` therefore drives every rule through the
real dispatcher instead.

This package is deliberately **not registered** in the root crate's `REGISTRY`.

## Why `match` reachability is worth a rule

Racket's `match` performs **no** reachability analysis, and `raco make` compiles
a dead clause without a word:

```racket
(match x
  [(? number?) 'num]
  [_           'other]
  [(? string?) 'str])   ; dead: (match "hi" ...) returns 'other
```

The rule found this in **Racket's own source** — `collects/setup/parallel-build.rkt`
lines 378-379 and 383-384, where `[x (format "DIDNT MATCH B ~v" x)]` is followed
by `[_ (send/error (format "DIDNT MATCH B\n"))]`. Reduced and run, the second
clause never fires.

The `else` spelling is the same defect in a `cond` costume: `match` gives `else`
no special meaning, so `[else …]` binds a variable named `else` and matches
everything. Verified, not assumed.

A clause guarded by `#:when` is **not** a catch-all, because the guard can fail
and fall through. That is the rule's sharpest suppression.

## Corpus audit

`tests/corpus_audit.rs` is the third-party false-positive sweep, `#[ignore]`d
because it needs a corpus that is not in the repository:

```text
PAREDIT_RACKET_CORPUS=/path/to/racket:/path/to/typed-racket \
  cargo test -p paredit-feature-lint-racket-depth --release \
  --test corpus_audit -- --ignored --nocapture
```

Result over `racket/racket` + `racket/typed-racket`, 4492 files:

| | |
| --- | --- |
| files found | 4492 |
| **files parsed** | **2715** |
| **parse failures** | **1777 (39.6%)** — see below |
| findings, all rules | 8 |
| false positives | 0 (one found and fixed) |

Candidate occurrences, counted independently of whether any rule fired, because
a zero-finding sweep over zero candidates is a false clean:

```text
define 11287   lambda 2930   let 2135   λ 1071   unless 962   when 721
match 351      parameterize 280   begin 261   case-lambda 144   begin0 56
let* 337       letrec 46     match-lambda 17    letrec* 6      match-lambda* 0
```

`match-lambda*` genuinely does not occur, and the audit asserts that explicitly
rather than pretending otherwise.

The one false positive was `(: f2 (case-lambda (Number * -> Number)))` in
`typed-racket-test/fail/cl-bug.rkt` — Typed Racket's legacy spelling of `case->`
as a **type**. Fixed by a node-local arrow test plus a type-position parent
check, both regression-tested from the corpus source.

## The parse gap this sweep exposed (`packages/core/syntax`)

**39.6% of the corpus does not parse.** This is a core-parser gap, not a rule
problem, and it caps what any Racket rule in this workspace can see. Grouped by
the reader construct at the failure offset:

| construct | files | what it is |
| --- | --- | --- |
| `#rx` / `#px` | 459 | regexp literals |
| `#<<` | 195 | here-strings |
| `#'` / `` #` `` | 267 | syntax quotes |
| `#%…` | ~290 | `#%kernel`, `#%app`, `#%module-begin` — pervasive in Racket |
| `#hash…` | 105 | hash literals |
| `#"…"` | 81 | byte strings |

All surface as `UnsupportedReaderDispatch { dispatch: "#" }`. Reported, not
fixed: this package does not modify `packages/core/**`.

## Cost

`src/cost_tests.rs`. Measured with `PassOptions { measure: true }` at two sizes
against a no-op control declaring the same heads (release, load average ~21):

```text
rule                                       ns/call@x1  ns/call@x8   ratio
cost-control-noop                                  20          19    0.96
racket-begin0-single-form                          25          21    0.85
racket-case-lambda-single-clause                   24          22    0.89
racket-for-comprehension-value-discarded           21          23    1.09
racket-match-unreachable-clause                    28          21    0.75
racket-parameterize-empty-bindings                 23          21    0.91
```

No quadratic behaviour: every rule is flat per invocation and within noise of
the control. That is by construction — each rule settles its node-local
questions first and asks `support::node_context` only about a node it would
otherwise report, so a clean file never pays for an ancestor walk.

A sixth rule, `racket-contract-out-arity-mismatch`, was built, tested, audited,
and then **dropped on these numbers** at 1.25 ms per realistic file with a
doubling ratio of 15.62. The evidence and the way to rebuild it are recorded in
`src/cost_tests.rs`.
