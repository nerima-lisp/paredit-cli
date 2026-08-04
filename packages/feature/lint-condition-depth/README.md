# paredit-feature-lint-condition-depth

Lint rules for the Common Lisp **condition designator protocol** and for
**`unwind-protect`'s interaction with a condition in flight** — two places where
the condition system behaves unlike the exception systems people arrive from,
and where the compiler says nothing.

`paredit-feature-lint-condition-system` already covers the *shapes*: a
`define-condition` with no `:report`, a `cerror` with no continue-format-control,
a `handler-bind` handler that ends in a discarded value. This package is about
what happens when a condition is actually constructed and signalled.

Status: **unregistered**. Both rules are complete, tested and audited, but are
not wired into the root registry.

## The rules

| Rule | Category | Severity | Fixability | Heads |
|---|---|---|---|---|
| `condition-type-datum-with-string-initarg` | `Conditions` | `Error` | `ReportOnly` | `error`, `cerror`, `signal`, `warn`, `make-condition` |
| `unwind-protect-cleanup-signals` | `Conditions` | `Warning` | `ReportOnly` | `unwind-protect` |

Both are `RuleDialectScope::COMMON_LISP_ONLY` (the trait default) and both use
`HeadFilter::Heads`; neither uses `WholeTree`.

### `condition-type-datum-with-string-initarg`

`(error 'my-error "boom ~A" x)` reads like "signal `my-error` with this message"
and is nothing of the kind. When the datum is a symbol naming a condition type,
the remaining arguments are **initarg names and values** (CLHS 9.1.4.1), so the
string lands in an initarg-*name* position. Verified against SBCL 2.6.0: an odd
argument count signals `simple-error` — "odd-length initializer list" — so the
named condition is never signalled at all; an even count is silently accepted and
discarded. `compile` reports `warnings-p=NIL failure-p=NIL` for both.

### `unwind-protect-cleanup-signals`

A cleanup form runs *during* the unwind, so an `error` signalled there replaces
the condition that caused the unwind. Verified against SBCL 2.6.0:
`(unwind-protect (error "ORIGINAL") (error "CLEANUP"))` escapes as `CLEANUP` and
the original is gone. Only a direct, reachable `error` is reported — `signal`,
`cerror`, `assert` and `check-type` are all continuable and are excluded, and the
walk stops at `handler-case`/`handler-bind`/`ignore-errors` and at any function
*defined* in the cleanup.

## The rule that was measured and rejected

`error-signalled-on-warning-condition` was designed, built, tested — and removed
on cost. Its premise held up well under SBCL (an `error` on a `warning` subtype
is not caught by `ignore-errors` and establishes no `muffle-warning` restart),
but deciding it needs the file's `define-condition` hierarchy, which cannot be
cached across `check()` calls. It measured **19–25 seconds on a zero-finding file
at n=2000**, against a shipped rule's ~1 ms in the same run. The shape is
preserved as `cost-control-file-hierarchy` in `cost_tests.rs`, with the numbers.

## Verification

- `realistic_corpus.rs` — correct Common Lisp yields zero findings **and**
  asserts a non-zero candidate count, paired with a dangerous twin that fires
  each rule exactly once.
- `corpus_audit.rs` — the third-party sweep. **1,619 files / 28.4 MB of SBCL's
  and Quicklisp's own sources, 2,705 candidate nodes, 0 findings**, with a
  self-test and an end-to-end planted-defect check so the zero can be believed.
- `cost_tests.rs` — invocation counts per rule, plus `#[ignore]`d doubling-ratio
  benchmarks against a no-op and a shipped-rule shape measured in the same run.

## Note on `support.rs`

`support.rs` copies the two-counter `QuoteState` quote model and
`is_unevaluated_at` from `paredit-feature-lint-condition-system::support`, as the
other lint packages do. A consolidation of that helper into `packages/core` is in
flight; when it lands, this module should be deleted and the shared one used
instead. A single `i32` depth counter is **not** an acceptable substitute — it is
wrong for `` ` ``/`,` and has shipped as a false-positive source twice; mutation
testing confirms six tests depend on the two-counter model.
