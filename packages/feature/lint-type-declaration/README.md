# paredit-feature-lint-type-declaration

Lint rules for Common Lisp's declaration system — `declare`, `declaim`, `the`,
and the type specifiers they carry. This is where "the compiler trusted me and I
was wrong" bugs live: a declaration is a *promise*, and an optimising compiler is
entitled to generate code that assumes it.

Five rules, every one Common Lisp only, every one `HeadFilter::Heads`, every one
report-only.

| Rule | Category | Severity | What SBCL 2.6.0 says |
|---|---|---|---|
| `declare-not-at-head-of-body` | `Malformed` | `Error` | `caught ERROR: There is no function named DECLARE` |
| `declaim-inside-body` | `Declaration` | `Warning` | `STYLE-WARNING: DECLAIM where DECLARE was probably intended` |
| `type-declaration-contradicts-initform` | `Declaration` | `Warning` | `WARNING: Constant 0 conflicts with its asserted type STRING` |
| `the-form-with-impossible-type` | `Declaration` | `Warning` | `WARNING: Derived type of (LIST 1 2) is (VALUES CONS &OPTIONAL), conflicting with its asserted type NULL` |
| `type-declaration-on-rest-parameter` | `Declaration` | `Warning` | `WARNING: Derived type of (SB-C:%LISTIFY-REST-ARGS ...) is (VALUES LIST &OPTIONAL), conflicting with its asserted type FIXNUM` |

Each rule's own module documents the CLHS section it rests on and the exact
expression that was run against SBCL to check it.

## The design rule this package is built around

**Decline every compound type specifier.** `(or null hash-table)` around a `nil`
initform is correct, extremely common code, and a type lattice that tried to
reason about compound specifiers would fire on it. So
[`support::type_excludes`] answers only "can this *atomic* specifier definitely
not contain this literal", for a list of specifiers whose membership is fully
enumerable, and says nothing about anything else. `t`, `atom`, `sequence`,
`array` and `vector` are deliberately unmodelled — a string *is* a vector and *is*
a sequence, and those are exactly the questions a linter gets wrong.

That costs findings. It is still the right trade, and the corpus audit is why.

## What the corpus audit changed

Run over **2217 third-party Common Lisp files** (SBCL's own `src/`, ASDF, and
Quicklisp distributions) containing 21239 `(declare`, 3979 `(declaim`, 3661
`(the ` and 8446 `&rest`/`&body` occurrences. The first pass produced 16
findings. Every one was a false positive, and each taught the package something
it did not know:

- **CLHS 3.2.3.1.** The body of a top-level `locally`, `macrolet` or
  `symbol-macrolet` is processed as *top level forms*, so a `declaim` there is an
  ordinary proclamation. SBCL relies on this in `constraint.lisp` and
  `target-unicode.lisp`. Those three heads were dropped from
  `declaim-inside-body`.
- **A reader conditional can be the head.** `globaldb.lisp` opens a definition
  with `(#+sb-xc-host cl:defmacro #-sb-xc-host sb-xc:defmacro …)`. The folded
  `#+` atom still normalizes to `defmacro`, so the head index dispatches, and the
  two conditional atoms shift every later index. The guard now scans from index
  zero rather than from the body start.
- **`#.` builds docstrings.** `save.lisp` writes `#.(format nil "…")` where a
  documentation string goes. Statically that is a `format` call, so the
  declarations after it read as displaced.

After those narrowings the audit reports **zero findings** over the same corpus,
while the package's dangerous-twin test proves each rule still fires.

## Cost

Every rule declares `HeadFilter::Heads`. None is about an absence with no head to
anchor on, so none needs `WholeTree` — which the `clean/forms/*` benchmark gate
measures on every file whether a rule matches or not.

`src/cost_tests.rs` measures per-rule ns/invocation and invocation counts at four
file sizes, and caught one real bug: [`support::is_unevaluated_at`] descends from
the file's *root*, and a linear `find` at that first level costs one pass over
every top-level form **per finding**. It measured 646ms at 500 reporting
definitions and 2442ms at 1000 — 3.8× the work for 2× the input. It now
binary-searches each level.

The residual **per-finding** cost (~1.2µs on a file where every form reports,
against ~30ns per declining invocation, and itself doubling as the file doubles)
is *not* this package's. Two rules with different heads, different analyses and
no shared code measure the same number and the same growth; what they share is
the engine's finding materialisation downstream of `sink.report`. See
`ignored_bench_the_per_finding_cost_is_shared_by_unrelated_rules`.

## Not shipped

Five candidates were investigated and dropped.

**`ignore-declared-variable-then-used` — a true duplicate.** `lint-convention`
already ships `ignore-declaration-conflict`
(`packages/feature/lint-convention/src/ignore_declaration_conflict.rs:33`), with
the same diagnosis, the same category, the same `Fixability`, and literally the
same worked example. It was built here anyway before the duplicate was found, so
the FP work is not wasted — see the note below.

The other four rest on premises that did not survive contact with SBCL:

- **`optimize-safety-zero`** — `(safety 0)` is a deliberate choice, and SBCL's
  own `constraint.lisp` sets it in a `locally` on purpose. Also adjacent to the
  shipped `contradictory-optimize`
  (`packages/feature/lint-convention/src/contradictory_optimize.rs:29`), which
  reports one quality named twice.
- **`ftype-declaimed-after-definition`** — the premise is that the compiler never
  sees the declaration. SBCL refutes it: a late `declaim` *does* constrain later
  calls, identically to an early one. The real effect is only that the body's own
  conflict is demoted from `WARNING` to `STYLE-WARNING`, which SBCL already
  reports itself.
- **`special-declaration-without-defvar`** — SBCL says nothing at all, because
  declaring a special defined in another file is the ordinary way to reference
  one. Not soundly decidable at file scope.
- **`inline-declaimed-for-recursive-function`** — real (SBCL notes
  `*INLINE-EXPANSION-LIMIT* (50) was exceeded`), but only an optimisation *note*,
  and correlating a `declaim` with its `defun` is the whole-file shape these cost
  tests exist to prevent.

### Handover: four false-positive classes in `ignore-declaration-conflict`

The duplicate was found *after* its false positives had been characterised, so
the evidence is recorded here rather than thrown away. Run over the same 2217
files, the shipped rule produces **21 findings**, and spot-checking says all of
them are false positives on code SBCL compiles clean:

- **Quoted/templated declarations.** `body_uses` walks with
  `view_query::for_each_subview`, which is unfiltered, so a `(declare (ignore
  ,@dummies))` inside a macro template is read as a real declaration. Five
  findings name variables literally spelled `,@ignore-list`, `,@dummies` and
  `,@ignored` (`ir1opt.lisp:2004`, `seqtran.lisp:3939`,
  `closer-clozure.lisp:28`).
- **Shadowing.** No rebinding check, so an inner `lambda` that rebinds the name
  counts as a use of the outer one. `insts.lisp:662` (`posn`) and
  `target-error.lisp:507` (`condition`) are both exactly this, and both are
  SBCL's own source.
- **Lisp-2 namespaces.** A symbol in operator position names a *function*.
  `asdf.lisp` declares a `&key` parameter `builtin-system-p` `ignore` and calls
  the accessor of the same name.
- **Macro arguments.** A macro decides whether to evaluate its arguments.
  Verified against SBCL: a macro that does not evaluate its argument produces no
  warning, one that does produces `reading an ignored variable`.

The corresponding guards, each with a test and each mutation-tested, are in
`support.rs` (`is_unevaluated_at`, `for_each_evaluated_subview_where`'s
`Position::is_operator`) and can be lifted across.
