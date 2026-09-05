# paredit-feature-lint-lfe-carp-depth

Lint rules for LFE, grounded in what LFE's own compiler rejects.

**The package ships two LFE rules and no Carp rule.** The name records the
ground it was sent to cover; the Carp half is documented below as surveyed and
deliberately empty, not forgotten.

| rule | category | severity | fixability | heads | dialects |
|---|---|---|---|---|---|
| `lfe-illegal-guard-call` | `Malformed` | `Error` | `ReportOnly` | `when` | `Lfe` |
| `lfe-clause-after-catch-all` | `DeadCode` | `Warning` | `ReportOnly` | `case`, `receive`, `match-lambda`, `defun` | `Lfe` |

Both rules set `dialect_scope()` from their own `domain::DIALECTS`, so the head
set and the dialect gate cannot drift apart. The trait default is
`COMMON_LISP_ONLY`, and a rule that forgets the override silently never runs.

## Why these two, and not the obvious ones

Every premise here was **run against LFE 2.2.0 on Erlang/OTP 27.3.4.15**, not
read off documentation. That oracle is what separated the two shipped rules
from the ones that were dropped.

`lfe-illegal-guard-call` reports only **module-qualified** calls. LFE also
rejects an unqualified call to a user function in a guard, and that is the more
common defect — but LFE expands macros *before* linting, and `clj.lfe` ships
predicates like `atom?` that expand to `is_atom` and uses them in its own
guards. Compiling

```lfe
(defmacro atom? (x) `(is_atom ,x))
(defun a ((x) (when (atom? x)) 'macro-ok) ((_) 'other))
```

produced **no diagnostic at all**. `binding_table()` is empty for LFE, so there
is no sound way to tell a macro from a function, and the unqualified form of
the rule would fire on every such use. Qualified calls have no such ambiguity:
`call` is a core form that cannot be redefined, and `mod:fun` is resolved as a
remote call rather than looked up in the macro environment.

`lfe-clause-after-catch-all` replicates LFE's *own* test for whether a `defun`
is single-clause or matching — `lfe_lib:is_symb_list`, which
`lfe_macro.erl:1257` calls an "educated guess". Two of its premises were
measured rather than reasoned about, and one of them contradicts Erlang:

- **A bare variable is a fresh binding, not a comparison.** In Erlang an
  already-bound pattern variable compares; in LFE it shadows and therefore
  always matches. Had the Erlang reading been right, no sound rule would have
  been possible.
- **A repeated variable constrains.** `((x x) 'same)` produces no dead-clause
  warning for the clause after it, so it is not a catch-all.

## The false positives the corpus audit removed

Two, both from LFE's own sources and its ecosystem, both fixed rather than
tolerated:

1. **`=:=` contains a colon.** Splitting a head on its first colon read it as
   module `=` calling function `=` — a false positive on the commonest
   comparison operator in LFE guards. Both halves must now spell unquoted
   atoms.
2. **`defsyntax` templates are not code.** LFE's `dev/test_macro.lfe:27` has
   `(case e (p . b) (_ (c-ond . c)))` inside a `defsyntax` rule, where `p` is a
   *pattern variable* that becomes a real pattern at expansion. Read as code it
   is a bare-variable catch-all, making the `_` after it look dead.
   `scm.erl:47-59` gives the exact family — `defsyntax`, `define-syntax`,
   `let-syntax`, `syntaxlet`, `syntax-rules` — and `support::node_context` now
   suppresses all of them.

A third was avoided by design: the audit's only third-party guard finding was
`(when ,(lanes.util:not-in 'method methods))` inside a `` ` `` template, where
the qualified call runs at *expansion* time to produce the guard rather than
inside it. The quote gate already suppressed it.

## Known limitations, stated rather than assumed away

- **Quasiquoted templates are suppressed**, so a dead clause written into a
  `` `(case ,x ,@clauses) `` template is not reported. Spliced clauses are
  invisible, so a catch-all that looks last may not be. This costs recall to
  buy precision on the shape LFE uses most.
- **Brackets are not read as calls.** LFE treats `[…]` and `(…)` as the same
  list, so `[lists:member x y]` in a guard really is illegal and really is
  missed. `defsyntax` patterns are bracket-spelled, and reading heads off them
  would manufacture operators out of pattern syntax.
- **No type reasoning.** The compiler also warns "cannot match because of
  different types/sizes" and "because its guard evaluates to `false`". Both
  need a type checker; `type_table()` is empty for LFE. In the corpus sweep
  these accounted for 18 and 18 warnings respectively, against 2 of the
  "previous clause always matches" kind that this rule models.
- **Unqualified illegal guard calls are not reported**, for the macro reason
  above. The corpus's one such site is `(when (if …))` — an unqualified core
  form.

## Cost

Release build, load average ~60–95 (this repository runs many agents in
parallel; read the ratios, not the absolutes).

On **clean** code — the case a user actually pays for, since findings are rare:

| rule | n=250 | n=500 | 2x ratio (linear = 200) |
|---|---|---|---|
| `cost-control-eager-root-view` | 1036301 ns/inv | 1814515 ns/inv | 350 |
| `lfe-illegal-guard-call` | 991 ns/inv | 942 ns/inv | 190 |
| `lfe-clause-after-catch-all` | 554 ns/inv | 386 ns/inv | 139 |
| `cost-control-noop` | 41 ns/inv | 35 ns/inv | 171 |

`cost-control-eager-root-view` is the **ordering mistake written out**: the
same work, with the document-wide `node_context` descent placed *before* the
cheap local check instead of after it. It costs ~1000x what the correctly
ordered rule does, reproducing at larger scale the 450843 ns/call versus
28 ns/call measured for the corresponding cheap-first ordering. Both rules
reach `node_context` only once a finding is otherwise ready to report, which is
why their columns sit next to the no-op's.

On a **pathological** file where every form carries a finding, both rules go
quadratic (~400 ratio, 160028–957017 ns/inv): `node_context` materializes the
document once per finding, so cost is findings x file size. Real code does not
look like this — the 917-file corpus produced zero findings — but the shape is
real and is recorded here rather than discovered later.

## Corpus audit

~140 third-party repositories, cloned fresh.

| | scanned | parsed | parse rate |
|---|---|---|---|
| LFE (`.lfe`) | 921 | 917 | **99.6%** |
| Carp (`.carp`) | 553 | 553 | **100%** |

Findings **through the real engine**: **0 guard findings over 39 candidates**,
**0 clause findings over 4284 candidates**. The candidate counts are what make
that a pass rather than a false clean — the rules were asked a real question
4323 times and answered "no" every time.

Cross-checked against the compiler itself: compiling the corpus with `lfec`
produced 2 "a previous clause always matches" warnings, both the same site in
two copies of one repository, and that site is type-based
(`(case (< a b) … ('true …) ('false …) …)` — booleans are exhausted, no
syntactic catch-all exists). So the rule has **no false positives and no false
negatives against its own stated premise** on this corpus.

## Carp: surveyed, and deliberately empty

**There is no Carp oracle.** Carp was removed from nixpkgs on 2026-02-05 and
could not be run, so no Carp premise here could be verified — only
grammar-derived, and every such claim is labelled so.

The corpus is real (553 files, 100% parse) so the denominator was available;
the *oracle* was not. The strongest candidate considered was "a compiled `defn`
body calling a `defndynamic`/`defdynamic`-only core function", which Carp's
evaluator does reject. It was dropped on measurement: Carp's `core/` defines 52
such names, and they include single-letter ones like `x` and `y`, so matching
them structurally flags ordinary parameters. That is a false-positive generator,
not a rule, and without a Carp binary there is no way to adjudicate the
difference.

Carp keeps its one shipped rule, `carp-deprecated-thread-macro`, in
`packages/feature/lint-carp-idiom`. Shipping a second one audited against
nothing would have been worse than shipping none.

## Registration

`ENTRIES` is `cfg(test)` and names both rules in registry order. The package is
not part of the built-in catalog; adding it requires updating the pinned rule
counts and lint goldens.

`support::QuoteState` is copied from
`packages/feature/lint-condition-system/src/support.rs`, as the other dialect
packages also copy it. It should move to a shared home; a consolidation is
not part of this package's scope. The copy here adds `is_pruned`, which distinguishes "do not
report on this node" from "do not walk this subtree" — a hard `'` prunes, a
`` ` `` does not, and collapsing the two made every unquoted call inside a
macro template invisible.
