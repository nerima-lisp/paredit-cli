# paredit-feature-lint-macro-authoring

Lint rules for Common Lisp **macro authoring correctness** — the parts of
writing a macro that are not hygiene.

Macro *hygiene* is already covered by `paredit-feature-lisp-analysis`'s
`macro_hygiene_report`: variable capture, multiple evaluation of an argument,
parameter reordering, deep quasiquote nesting and a missing `declare`. Those are
about what the *expansion* does to the caller's names and values, and all five
are `Severity::Warning` because, as that package puts it, a hygiene risk is a
fact about the file rather than a defect by definition.

The two rules here are defects.

| rule | category | severity | fixability | heads |
| --- | --- | --- | --- | --- |
| `macro-body-destroys-argument-form` | `Suspicious` | `Error` | `ReportOnly` | `defmacro`, `define-compiler-macro` |
| `macrolet-expander-captures-lexical-variable` | `Suspicious` | `Error` | `ReportOnly` | `macrolet` |

Both are `RuleDialectScope::COMMON_LISP_ONLY` and both declare
`HeadFilter::Heads`.

## What each one is

**`macro-body-destroys-argument-form`** — a macro expander applying a
destructive operator (`nreverse`, `sort`, `nconc`, `remf`, `(setf (car …))`, …)
directly to one of its own parameters. Those parameters are bound to the
caller's *source*: `&body` is a tail of the list the reader built for the call
site. Destroying it edits the program in place. SBCL 2.6.0 emits **no
diagnostic at all**, and the second expansion of the call site produces a
different program.

**`macrolet-expander-captures-lexical-variable`** — a `macrolet` expander that
*evaluates* a name an enclosing form binds lexically. CLHS (`flet, labels,
macrolet`) makes the consequences undefined. The discriminator is one comma:
a name written plainly in the template is part of the expansion and is fine,
which is the commonest `macrolet` idiom there is; a name under a `,` is read by
the expander before the binding exists.

## What was dropped, and why

Seven rules were proposed. Five were dropped against SBCL 2.6.0 and CLHS:

- **every macro-lambda-list marker misplacement** (`&whole` not first,
  `&environment` misplaced or repeated, `&body` with `&rest`) — SBCL rejects
  all of them with a hard `SIMPLE-PROGRAM-ERROR` at `defmacro` processing time.
  The file does not load. CLHS 3.4.4 also **refutes** two of the premises
  outright: `&environment` "can appear anywhere in that list", and `&body` has
  no "must be last" rule.
- **`macro-returns-non-form`** — `(defmacro version () "1.0")` is legal, works,
  and is occasionally deliberate.
- **`macroexpand-not-idempotent-marker`** — only same-arity self-expansion
  loops, and no real code writes it.
- **`quasiquote-splice-of-non-list`** — `` `(a ,@5) `` is *legal* and yields the
  dotted list `(A . 5)`; only the non-final position errors, and only at run
  time.
- **`compiler-macro-disagrees-with-function`** — the premise held (an
  incongruent compiler macro is a *silent* dead optimization), but it is a
  cross-form correlation and was **dropped on measurement**, at a doubling ratio
  of 170 where linear is 8. See `src/cost_tests.rs`, which keeps the control
  that measured it so the decision can be re-checked.

## Evidence

Every premise was run against SBCL 2.6.0 rather than assumed, and every rule
was swept over code it did not choose: SBCL 2.6.6's own sources and
`~/quicklisp/dists/quicklisp/software` — **1297 files, 2124
`defmacro`/`define-compiler-macro` candidates, 966 `macrolet` candidates**,
yielding **one** finding, which is a true positive in SBCL's own
`src/compiler/type-vop-macros.lisp:181`.

- `src/support.rs` — the shared two-counter quote model and the root descent.
- `src/corpus.rs` — the permanent clean/dangerous pair.
- `src/corpus_audit.rs` — the third-party sweep, `#[ignore]`d.
- `src/cost_tests.rs` — what each rule costs, and the rule that cost too much.

The package is deliberately left **unregistered**: the root `REGISTRY` is wired
in a separate pass.
