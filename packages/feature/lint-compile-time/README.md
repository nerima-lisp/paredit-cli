# paredit-feature-lint-compile-time

Lint rules for the Common Lisp compile-time / load-time phase distinction.

## Responsibilities

Three rules about the gap between what a file does when you `load` it and what
it does when you `compile-file` it — the gap the phrase "it works in the REPL
but not from a compiled file" names.

| Rule | Flags |
| --- | --- |
| `eval-when-execute-only` | a **top level** `eval-when` naming `:execute` but neither `:compile-toplevel` nor `:load-toplevel`, wrapping a definition — `compile-file` discards the body entirely |
| `eval-when-body-never-runs` | a **non**-top-level `eval-when` naming only situations the standard ignores there, so its body never runs in any phase |
| `defconstant-non-eql-value` | a `defconstant` whose initform allocates, so the compile-time and load-time values are not `eql` |

That list is the package's real specification: §5.2.2 splits by subject matter,
so naming the rules is the only way to say why one belongs here.

Every rule is Common Lisp only (the default `RuleDialectScope`), every rule is
`Fixability::ReportOnly`, and every rule is `HeadFilter::Heads` — never
`WholeTree`, never `AllNodes`. No rule touches
`RuleContext::binding_table`/`value_table`/`type_table`, and none touches
`RuleContext::scratch_cache` (see below).

| Rule | Category | Severity | Heads |
| --- | --- | --- | --- |
| `eval-when-execute-only` | `Suspicious` | `Error` | `["eval-when"]` |
| `eval-when-body-never-runs` | `DeadCode` | `Error` | `["eval-when"]` |
| `defconstant-non-eql-value` | `Suspicious` | `Error` | `["defconstant"]` |

The two `eval-when` rules share a head and are complements, not duplicates: one
fires only at top level and only when both top-level situations are absent, the
other only away from top level and only when `:execute` is absent. No form can
satisfy both, and `eval_when_body_never_runs/rule.rs` pins that with a test
that runs both rules over nine shapes and asserts at most one finding each.

## What every claim here was checked against

Every premise was run through SBCL 2.6.0 under **both** `load` of the source and
`compile-file` followed by `load` of the fasl, in a fresh subprocess per case.
The three rules that shipped are the ones where those two phases disagree, or
where the form is dead in both and nothing says so.

| shape | `load` source | `compile-file` + load fasl |
| --- | --- | --- |
| `(eval-when (:execute) (defmacro m …))` | works | **undefined function** at run time |
| `(eval-when (:load-toplevel :execute) (defmacro m …))` | works | works |
| `(eval-when (:compile-toplevel :load-toplevel :execute) …)` | works | works |
| `(defun f () (eval-when (:compile-toplevel) (setf *m* :fired)))` | **never runs** | **never runs** |
| `(defconstant +x+ #("a" "b"))` | works | **`DEFCONSTANT-UNEQL`** |

The third row is why neither rule keys on "the situation list is missing
`:compile-toplevel`", which is the obvious predicate and is wrong: `defmacro`'s
own expansion carries an inner `(eval-when (:compile-toplevel) …)`, and CLHS
3.2.3.1 keeps the body of a top-level `eval-when` top level, so the inner one
runs at compile time regardless. A rule written against the obvious predicate
would fire on every `(eval-when (:load-toplevel :execute) …)` in the world.

## Top level is a recursion, not a depth

CLHS 3.2.3.1 defines a top level form by recursion: the body of a top-level
`progn`, `locally`, `macrolet`, `symbol-macrolet` or `eval-when` is itself
processed as top level. `support::is_top_level_form` enumerates those five
operators, and enumerates the child index at which each one's *body* begins —
an `eval-when`'s situations list and a `macrolet`'s bindings list are inside the
form but are not body, and a candidate found in one of them is not a top level
form.

Both halves matter. An incomplete operator list produces false positives on
`(locally …)` and `(macrolet () …)`. A wrong body index is subtler: it only shows up
when a non-body position happens to contain a list whose head is itself one of
the five, and mutation testing caught that the obvious test cases never reach
it.

## Cost

`HeadFilter::Heads` throughout, so the `clean/forms/*` benchmark — whose 10%
threshold has failed this project five times — dispatches **nothing**. Measured:
0 invocations of all three rules over a 200-unit and a 400-unit clean corpus
containing no `eval-when` and no `defconstant`. That is a structural result, not
a timing one: the head index answers before `check` is reached.

Inside `check`, every rule answers a **node-local** question first — the
situation list, or the initform's shape — and only a node that has already
failed that reaches `is_top_level_form`, which materializes the enclosing
top-level form. That ordering is the whole cost model. A sibling package
measured 450843 ns/call against 28 ns/call purely from asking the tree question
before the cheap one, and each rule's `rule.rs` restates its own ordering.

`is_top_level_form` binary-searches the top level using
`SyntaxTree::root_child_span`, which is an index into a slice and a field read.
The equivalent-looking `select_path(&Path::root_child(i))?.span()` builds an
`ExpressionPath`, which owns a `Vec`, so it would heap-allocate on every step of
the search rather than once at the end.

Measured on a 131050-byte file with 400 `eval-when` and 400 `defconstant` forms,
debug profile, against shipped rules on the identical file:

| rule | ns/invocation | doubling ratio |
| --- | --- | --- |
| `defconstant-non-eql-value` | 195 | ×1.94 |
| `eval-when-body-never-runs` | 518 | ×1.96 |
| `eval-when-execute-only` | 547 | ×1.94 |
| *shipped* `self-recursive-tail-call` | 244 | — |
| *shipped* `macro-deep-quasiquote-nesting` | 13445 | — |
| *shipped* `duplicate-defmethod-signature` | 1248688 | — |

## `RuleContext::scratch_cache` is not available to this package

It looks like the right home for a shared per-file computation. It is not
usable: the slot holds **one type per file's pass**, and
`paredit-feature-lint-repl-debug` already stores its evaluated-forms walk there
(`packages/feature/lint-repl-debug/src/support.rs:612`). A second caller with a
different `T` *panics* rather than missing the cache, and `inspect lint` runs
every rule on every file, so the two would meet on the first file with both a
candidate here and a REPL-debug candidate. Any future rule here that wants a
per-file table must pay its own way or promote the slot to a `TypeId`-keyed map
first.

## What this package does not own, and four rules deliberately not written

Each of these was proposed, investigated against SBCL 2.6.0, and **dropped** on
the evidence.

- **No `macro-used-before-defined-in-file` rule.** The premise was that a
  `defmacro` below its call site works under `load` and breaks under
  `compile-file`. Measured: it breaks under **both**, identically, and SBCL
  names the cause precisely in both — `MY-MAC is being redefined as a macro when
  it was previously assumed to be a function`, followed by a hard
  `UNDEFINED-FUNCTION` at run time. A lint rule would restate a diagnostic the
  compiler already gives at the exact source location, and would have to guess
  whether a call names a same-file macro or a function from another file.
- **No `defmacro-without-eval-when` rule.** This one is real and is decidable in
  a narrow form — a helper called from a macro body's *evaluated* position, as
  opposed to its quasiquote template, which the two-counter quote model
  separates exactly. It was dropped on value, not on soundness: SBCL raises a
  hard `ERROR` that *fails the compilation* and says `The function HELPER is
  undefined. It is defined earlier in the file but is not available at
  compile-time.` The defect is also unobservable unless the macro is used in the
  same file — across files the helper's fasl is already loaded — and when it is
  used in the same file, the compiler always sees it. Its true-positive set is
  exactly the set already caught loudly.
- **No `defpackage-not-first-form` rule.** Premise refuted. `(defun early () 1)`
  before `(defpackage #:p (:export #:early))` still interns `P:EARLY` as
  `:EXTERNAL` in both phases — the earlier `defun` interned into a *different*
  package, and `defpackage` creates a new one. "Not the first form" is not the
  question; there is no defect here to ask about. `defpackage-without-in-package`
  in `feature/lint-build-system` owns the file-scope question that is real.
- **No `read-time-eval-with-side-effect` rule.** Measured: `#.` fires exactly
  once in each phase and produces the same value in both. There is no phase
  disagreement to report, and `#.` is deliberate by construction. A rule whose
  advice is "do not do that" is not worth a name. (`load-time-value` in a macro
  body was investigated with it and dropped for the same reason: it fails
  loudly and identically in both phases.)

Beyond those:

- **No registry.** `REGISTRY` stays in the root and names each rule's `META` and
  `RULE` across this boundary. A registry here would be the cycle §4.2 exists to
  prevent. **This package is deliberately unregistered**; a separate pass wires
  it.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No rule about `declaim` placement.** `declaim-inside-body` in
  `feature/lint-type-declaration` owns that, and it is a question about
  declaration scope rather than about evaluation phase.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`. |
| `paredit-core-syntax` | Rules match on parsed forms and on the shared `definition` classifier. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

No `paredit-feature-*` dependency, not even a dev one:
`tests/cli/feature_dependency_contract.rs` scans manifests as whole text, so a
dev-dependency on another feature package would fail that contract exactly like
a real one.

## Layout

One rule, one directory — the four files a rule is made of, plus two shared
modules:

```text
src/
├── support.rs        quote model, CLHS 3.2.3.1 top level, eval-when situations
├── corpus_tests.rs   the permanent correct/dangerous corpus pair
└── <rule>/
    ├── rule.rs       META, RULE, the head filter: what the registry registers
    ├── domain.rs     the detection itself
    ├── usecase.rs
    └── cli/          the `inspect <rule>` subcommand
```

`support.rs`'s quote machinery is a deliberate **copy** of
`feature/lint-condition-system`'s, not a dependency on it — the same copy
`feature/lint-build-system` keeps, and for the same reason: two packages of lint
rules should not couple, and the semantics are the part worth sharing. Two
independent counters, because `'` and `` ` `` are not the same thing.

That is not incidental here. A `defmacro` whose template emits
`(eval-when (:compile-toplevel) …)` for the *caller's* file is
`eval-when-body-never-runs`'s exact shape and is correct code; the audit found
**30** such forms, and every one is a false positive for any implementation that
walks data. All five quote shapes are pinned by tests in `support.rs` and again
in each rule's own tests, and mutation testing confirms that collapsing the two
counters into one breaks them.

## The corpus audit

Run over 1619 files of Common Lisp nobody involved here wrote — SBCL 2.6.0's own
sources and every unpacked release under `~/quicklisp/dists/`.

| | |
| --- | --- |
| files scanned | 1588 |
| files that failed to parse (reported, not skipped) | 31 |
| `eval-when` forms reached as code | 347 |
| `eval-when` nodes suppressed as quoted data | 133 |
| `defconstant` forms reached as code | 1079 |
| findings | **9**, all `defconstant-non-eql-value` |
| false positives | **0** |
| false positives the quote model prevented | 30 |

All 9 findings were adjudicated by extracting the exact form into a standalone
file and running `compile-file` + `load` of the fasl in a fresh SBCL: every one
reproduces `DEFCONSTANT-UNEQL`. The two `eval-when` rules found nothing because
the corpus contains no `execute`-only `eval-when` at all and exactly one nested
one, which correctly names `:execute` — a true clean over a real denominator,
not a silent rule.

the findings precisely because a zero-finding sweep over zero candidates is a
false clean.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about when a form is evaluated relative to `compile-file` | it is a new slice here, plus one line in the root's REGISTRY |
| changing what one of the three flags, or how it phrases it | that rule's `domain.rs` |
| changing which forms a rule is shown | that rule's `rule.rs` head filter |
| teaching the rules a new allocating constructor or `eval-when` spelling | `support.rs`, or `defconstant_non_eql_value`'s `ALWAYS_FRESH` table |

| You are… | and it does **not** belong here because… |
| --- | --- |
| writing a rule about where `declaim` may appear | that is `feature/lint-type-declaration` |
| writing a rule about a file that declares a package and never enters it | that is `feature/lint-build-system` |
| writing a rule about macro hygiene or variable capture | that is `feature/lisp-analysis`'s `macro_hygiene_report` |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
| changing `inspect lint` itself | that is the root, which owns the registry |

Adding a dependency to `Cargo.toml` means adding a row to the table above.
