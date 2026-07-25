# Command model

Every source-facing command belongs to one of three namespaces. This gives
automation a stable first decision: inspect, edit, or refactor.

- `paredit inspect` reads and reports without writing.
- `paredit edit` transforms one selected form; stdout by default, `--diff`
  for a unified diff, `--write` to update the file in place.
- `paredit refactor` plans, previews, verifies, and applies semantic changes.

The only command outside these namespaces is the `paredit completions
<shell>` meta command, which prints shell completion scripts for bash, zsh,
fish, elvish, and powershell.

Run `paredit <namespace> --help` for the authoritative list on your installed
version, and `paredit <namespace> <command> --help` for each command's
contract, arguments, and output formats. For a machine-readable catalog of
the entire surface in one call, run:

```sh
paredit inspect capabilities --output json
```

## Inspect

`paredit inspect` never writes source files. Prefer these commands for
discovery, impact analysis, and preflight checks.

| Command | Purpose |
| --- | --- |
| `check` | Validate that input is a balanced S-expression document. |
| `dialect` | Detect Lisp dialect from `--file` extension or explicit `--dialect`. |
| `stats` | Print parse, dialect, and structural metrics for agent planning. |
| `agent-report` | Print a complete JSON report for AI coding agent refactor planning. |
| `capabilities` | Print a machine-readable catalog of every command, flag, default, and enum value. |
| `outline` | Print top-level forms with paths, spans, and definition hints. |
| `form` | Report one selected form with local structure for refactor planning. |
| `find-symbol` | Find exact atom occurrences without touching strings or comments. |
| `symbols` | Report exact atom occurrences across explicit files for rename planning. |
| `calls` | Report list-head call sites across explicit files for arity refactor planning. |
| `signature` | Compare callable definitions and call-site arity across explicit files. |
| `call-graph` | Report internal and optional external call graph edges. |
| `impact` | Report refactoring impact risks for one symbol across explicit files. |
| `workspace` | Discover Lisp sources under roots and report parse/refactor inventory. |
| `dependencies` | Report package, system, load, and qualified-symbol dependencies. |
| `packages` | Report Common Lisp package declarations across explicit files. |
| `definitions` | Report definition-like top-level forms across explicit files. |
| `unused-definitions` | Report definitions with no external exact atom references. |
| `duplicates` | Report repeated structural S-expression shapes across explicit files. |
| `similarity` | Report structurally similar S-expression forms across explicit files. |
| `lets` | Report local let bindings and inline safety for refactor planning. |
| `complexity` | Report per-definition nesting depth and size metrics for refactor prioritization. |
| `naming` | Report definition names that deviate from idiomatic kebab-case Lisp naming. |
| `reachability` | Report callable definitions unreachable from any entry point in the internal call graph. |
| `unused-parameters` | Report declared function parameters with no unshadowed reference in their body. |
| `shadowed-bindings` | Report let-family bindings that shadow an enclosing parameter or let binding. |
| `unused-local-callables` | Report flet/labels local callables never called anywhere in their visible scope. |
| `package-boundaries` | Report `package::symbol` references that reach into another package's internal symbols. |
| `call-cycles` | Report strongly connected cycles of two or more definitions in the internal call graph. |
| `package-cycles` | Report defpackage :use/:import-from cycles across two or more packages. |
| `system-cycles` | Report ASDF defsystem :depends-on cycles across two or more systems. |
| `unused-packages` | Report defpackage declarations never used, imported-from, or reached by a qualified symbol. |
| `unused-exports` | Report defpackage :export symbols never reached by a qualified symbol reference. |
| `duplicate-exports` | Report defpackage forms that export the same symbol more than once. |
| `unused-nicknames` | Report defpackage :nicknames never used as a qualifier anywhere. |
| `package-conflicts` | Report distinct defpackage forms that claim the same package name or nickname. |
| `redefinitions` | Report top-level definitions of the same category and name declared more than once. |
| `undefined-packages` | Report in-package forms naming a package no analyzed defpackage declares. |
| `class-cycles` | Report CLOS defclass/define-condition superclass inheritance cycles across two or more classes. |
| `struct-cycles` | Report defstruct :include cycles across two or more structs. |
| `system-conflicts` | Report distinct asdf:defsystem forms that claim the same system name. |
| `duplicate-slots` | Report defclass/define-condition/defstruct forms declaring the same slot name more than once. |
| `duplicate-methods` | Report defmethod forms with the same name, qualifier, and specializers declared more than once. |
| `duplicate-parameters` | Report callable definitions whose lambda list names the same parameter more than once. |
| `redundant-quote` | Report self-evaluating literals (numbers, strings, characters, keywords) that are quoted redundantly. |
| `redundant-progn` | Report progn forms that are redundant — empty (`(progn)` is `nil`) or wrapping a single form (`(progn X)` is just `X`). |
| `redundant-prog1` | Report a `(prog1 x)` wrapping a single form; `prog1` returns its first form's value, so with one form it is just `x`. Auto-fixable (unwraps to `x`). A multi-form `(prog1 x y)` (which sequences side effects and cannot become `progn`) and an empty `(prog1)` are left alone. |
| `negated-when-unless` | Report when/unless forms whose test is a `(not X)`/`(null X)` negation; flipping the macro (`when`↔`unless`) and dropping the negation reads more directly. |
| `nested-progn` | Report a multi-form `progn` nested directly inside another `progn`; its forms splice into the outer one, so `(progn a (progn b c) d)` is just `(progn a b c d)`. |
| `redundant-body-progn` | Report a multi-form `progn` used as the body of a form that already has an implicit progn (`when`, `unless`, `let`, `defun`, `lambda`, …), so `(when c (progn a b))` is just `(when c a b)`. |
| `empty-let` | Report a `let` whose binding list is empty (`(let () body)` or `(let nil body)`); with no bindings, it is just `(progn body)`. Auto-fixable (rewrites `(let ()` as `(progn`). A body that leads with `(declare …)` is left alone (invalid in `progn`), as is `(let* () …)` — the province of `redundant-let-star`. |
| `redundant-if-nil` | Report a three-argument `if` whose else branch is a literal `nil`; a two-argument `if` already returns `nil`, so `(if c x nil)` is just `(if c x)`. |
| `redundant-let-star` | Report a `let*` whose binding list holds zero or one binding: with no earlier binding to depend on, the sequential scope is unused, so `(let* ((x e)) body)` is just `(let ((x e)) body)`. Auto-fixable (rewrites the head to `let`). |
| `single-clause-cond` | Report a `cond` with exactly one clause that has a body and a non-`t` test; with nothing to fall through to, `(cond (test a b))` is just `(when test a b)`. Auto-fixable (rewrites to `when`). A `t`/`otherwise` catch-all clause (a `progn`) and a test-only clause (returns the test value) are left alone. |
| `cond-t-clause` | Report a `cond` with exactly one clause whose test is the literal `t` and which has a body; since `t` always holds and there is nothing to fall through to, `(cond (t a b))` is just `(progn a b)`. Auto-fixable (rewrites to `progn`). The `t`-clause complement of `single-clause-cond`. A multi-clause `cond` (a trailing `(t …)` else is idiomatic), a test-only `(cond (t))` (returns `t`), and a non-`t` test are left alone; `otherwise` is not special in `cond` and is not treated as a catch-all. |
| `explicit-step-delta` | Report an `incf`/`decf` whose delta operand is the literal `1`; since `1` is the default step, `(incf x 1)` is just `(incf x)` (and `(decf x 1)` is `(decf x)`). Auto-fixable (drops the delta). Only the bare integer `1` is matched — a float `1.0` can coerce the place's type, so it is left alone. |
| `negated-step-delta` | Report an `incf`/`decf` whose delta is a *negative* numeric literal; adding a negative delta is subtracting, so `(incf x -1)` is `(decf x 1)` and `(decf n -5)` is `(incf n 5)`. Auto-fixable (flips the operator and drops the sign); a resulting `(decf x 1)` is then reduced to `(decf x)` by `explicit-step-delta`. A variable delta and a positive literal are left alone. |
| `explicit-nil-return` | Report a `return` or `return-from` whose result form is the literal `nil`; `nil` is the default result, so `(return nil)` is just `(return)` and `(return-from foo nil)` is `(return-from foo)`. Auto-fixable (drops the `nil`). `(return-from nil)` is left alone — there the `nil` is the block name, not a result. |
| `funcall-lambda` | Report a `funcall` whose first argument is a literal `(lambda …)` form; a lambda form is directly applicable, so `(funcall (lambda (x) …) a)` is just `((lambda (x) …) a)`. Auto-fixable (drops `funcall`). The `#'symbol` case belongs to `redundant-funcall`; a `#'(lambda …)` first argument is left alone (its reader prefix cannot sit in operator position). |
| `if-to-or` | Report a three-argument `if` whose test and then-branch are the same bare atom; `(if x x y)` returns `x` or the else, which is `(or x y)` — evaluated once instead of twice. Auto-fixable (rewrites to `or`). Only an atom test/then pair is matched (a compound test would be evaluated twice); a literal `t`/`nil` test belongs to `constant-if-test`. |
| `if-not` | Report a three-argument `if` whose then-branch is the literal `nil` and else-branch is the literal `t`; `(if test nil t)` yields `nil` when `test` holds and `t` otherwise, which is exactly `(not test)`. Auto-fixable (rewrites to `(not test)`, copying the test verbatim). The dual `(if test t nil)` is a boolean coercion with no clearer builtin and is left alone, as is `(if test nil nil)` (`identical-if-branches`) and any non-literal branch. |
| `one-step-arithmetic` | Report a two-argument `+`/`-` of the literal `1`, which has a unary shorthand: `(+ x 1)` and `(+ 1 x)` are `(1+ x)`, and `(- x 1)` is `(1- x)`. Auto-fixable (rewrites to `1+`/`1-`). `(- 1 x)` has no shorthand and is left alone; only the bare integer `1` matches (a float `1.0` would coerce the result type). |
| `case-nil-key` | Report a `case`/`ecase`/`ccase` clause whose key is the bare atom `nil`. `case` keys designate a list of objects, and `nil` designates the *empty* list, so `(case x (nil …))` can never match — to match the value `nil` you must write `((nil) …)`. Report-only (an **error**-severity likely bug). `((nil) …)` and a quoted `'nil` (see `quoted-case-key`) are left alone. |
| `typecase-nil-key` | Report a `typecase`/`etypecase`/`ctypecase` clause whose head is the bare atom `nil`. In `typecase` a clause head is a *type specifier*, and `nil` is the empty type — no object is of it — so `(typecase x (nil …))` is a dead clause. To match the `nil` value use the `null` type. Report-only (**error**-severity). The `t` catch-all, the `null` type, and a quoted `'nil` are left alone. The typecase-family analog of `case-nil-key`. |
| `sharp-quoted-lambda` | Report a `(lambda …)` form with a redundant `#'` prefix. `lambda` already expands to `(function (lambda …))`, so `#'(lambda (x) …)` is exactly `(lambda (x) …)` in every position. Auto-fixable (strips the `#'`). A `#'foo` symbol reference and a bare `(lambda …)` are left alone. |
| `redundant-eql-test` | Report an explicit `:test #'eql` on an operator whose `:test` already defaults to `eql` (`member`, `assoc`, `find`, `position`, `count`, `remove`, the set operations, …), so `(find x list :test #'eql)` is just `(find x list)`. Auto-fixable (deletes the `:test #'eql` pair). Recognizes `#'eql`, `'eql`, and `(function eql)`; a custom test, `:test-not`, and non-eql-defaulting operators are left alone. |
| `redundant-identity-key` | Report an explicit `:key #'identity` (or `:key nil`) on an operator that accepts `:key` (`find`, `position`, `remove`, `sort`, `merge`, `reduce`, the `-if` variants, …); `:key` defaults to `nil` (the element itself), so `(sort xs #'< :key #'identity)` is just `(sort xs #'<)`. Auto-fixable (deletes the pair). Recognizes `#'identity`, `'identity`, `(function identity)`, and `nil`; a custom key and non-`:key` operators (e.g. `tree-equal`) are left alone. |
| `single-value-bind` | Report a `multiple-value-bind` that binds exactly one variable; `let` captures the form's primary value, which is all a one-variable bind uses, so `(multiple-value-bind (x) f body)` is just `(let ((x f)) body)`. Auto-fixable (rewrites to `let`). Two-or-more variables (which capture secondary values) and an empty variable list (a `progn`) are left alone. |
| `nested-boolean` | Report an `and`/`or` nested directly inside a same-operator `and`/`or`; both operators are associative with identical short-circuiting, so `(or a (or b c) d)` is just `(or a b c d)` (and likewise for `and`). Auto-fixable (splices the inner operands in). The inner form must have two or more operands — the single-operand `(or x)` collapse belongs to `single-operand-boolean`. |
| `nested-when` | Report a `when` whose only body form is another `when`; the two tests combine, so `(when a (when b body))` is `(when (and a b) body)`. Auto-fixable (merges the tests with `and`). An outer `when` with additional body forms after the inner `when` is left alone — those forms are not guarded by the inner test. |
| `nested-unless` | Report an `unless` whose only body form is another `unless`; the body runs only when both tests are nil, so `(unless a (unless b body))` is `(unless (or a b) body)`. Auto-fixable (merges the tests with `or`). The `or`-combining mirror of `nested-when`; an outer `unless` with extra body forms is left alone. |
| `empty-body` | Report a `when`/`unless`/`dolist`/`dotimes` form that has its test/spec but no body (`(when ready)`, `(dolist (x items))`); the test/spec runs and then nothing happens — usually a forgotten body. |
| `verbose-negation` | Report negation written the long way: `(- 0 x)` (zero minus x), `(* x -1)` and `(* -1 x)` (times minus one) are all `(- x)`. Auto-fixable (rewrites to unary `(- x)`). Only bare integer `0`/`-1` are matched (a float `-1.0` would coerce the result type); the *trailing* `(- x 0)` is `identity-arithmetic`'s job. |
| `identity-arithmetic` | Report an arithmetic form with a redundant integer identity operand (`(+ x 0)`, `(* x 1)`, `(- x 0)`, `(/ x 1)`); adding 0, multiplying by 1, etc. does nothing. Float `0.0`/`1.0` (which coerce) and the leading operand of `-`/`/` are not flagged. |
| `redundant-divisor` | Report a two-argument `floor`/`ceiling`/`truncate`/`round` (or the float variants `ffloor`/`fceiling`/`ftruncate`/`fround`) whose divisor is the literal integer `1`; the divisor defaults to `1`, so `(floor x 1)` is exactly `(floor x)` — same quotient and remainder. Auto-fixable (drops the divisor, preserving operator casing). A float `1.0` (which changes the remainder type), a non-`1` divisor, and `mod`/`rem` (no defaultable divisor) are left alone. |
| `single-operand-list-op` | Report a single-argument `append`, `nconc`, or `list*`, each of which returns its argument unchanged, so `(append x)` is just `x`. Auto-fixable (replaces the form with the argument). Scoped to these three because their single-argument identity is unconditional; numeric ops like `(max x)` (which require a real and would signal a type error) are excluded. Two-or-zero-argument forms are left alone. |
| `single-operand-arithmetic` | Report a single-operand `+`/`*` form (`(+ x)`, `(* x)`); a one-argument `+`/`*` returns its operand verbatim, so the wrapper is pure noise. Auto-fixable (unwraps to the operand). Unary `(- x)` (negation) and `(/ x)` (reciprocal) are meaningful and not flagged; the zero-operand identities `(+)`/`(*)` are left alone. |
| `redundant-funcall` | Report a `funcall` of a sharp-quoted symbol (`(funcall #'foo a b)`); it resolves `foo` through the same lexical function namespace as a direct `(foo a b)`, so the `funcall`/`#'` is ceremony. Auto-fixable (rewrites to the direct call). A variable argument (`(funcall fn …)`), a sharp-quoted lambda (`#'(lambda …)`), and an ordinary-quoted symbol (`'foo`) are not flagged. |
| `redundant-the` | Report a `(the t form)` type declaration; the type `t` matches every object, so the assertion is vacuous and the form is exactly `form` (`the` passes all values through, so this holds for multiple values too). Auto-fixable (replaces the form with its inner form). A specific type (`(the fixnum x)`), a compound type, and a wrong-arity `(the t)`/`(the t a b)` are not flagged. |
| `nil-comparison` | Report an `eq`/`eql`/`equal`/`equalp` comparison against a bare `nil` (`(eq x nil)`, `(equal nil y)`); each is exactly the idiomatic `(null x)` nil test. Auto-fixable (rewrites to `(null X)`). Numeric `=` (a type error on nil), a degenerate `(eq nil nil)`, and a quoted `'nil` are not flagged. |
| `one-armed-if` | Report a two-argument `if` with no else branch (`(if test then)`); the mainstream style guides recommend `(when test then)` for a conditional with a single arm. Auto-fixable (swaps the `if` head for `when`; a `progn` then-branch is then spliced by `redundant-body-progn`). A two-armed `(if test a b)`, an argument-short `(if test)`, and a reader-conditional operand are not flagged. |
| `t-comparison` | Report an `eq`/`eql`/`equal`/`equalp` comparison against the literal `t` (`(eq x t)`); it matches only the symbol `T`, so as a boolean test it silently fails for any other true value (a generalized-boolean mistake). Report-only — the right rewrite (drop the comparison, or keep an intentional symbol test) depends on intent. The symmetric partner of `nil-comparison`. Numeric `=`, a degenerate `(eq t t)`, and a quoted `'t` are not flagged. |
| `manual-incf` | Report a `setf`/`setq` that manually increments or decrements a variable (`(setf x (1+ x))`, `(setq n (+ n 2))`, `(setf i (- i 1))`), which is exactly what `incf`/`decf` express. Auto-fixable (rewrites to `(incf x)` / `(incf n 2)` / `(decf i 1)`). Only bare-variable places are matched (so `incf`'s single evaluation of a compound place never changes side-effect timing); a compound place, a non-commuting `(- d v)`, a different variable, and a multi-pair `setf` are not flagged. |
| `manual-push` | Report a `setf`/`setq` that manually conses an element onto a variable (`(setf stack (cons item stack))`), which is exactly what `push` expresses. Auto-fixable (rewrites to `(push item stack)`). Because `cons` is `(cons element list)`, only a form whose *second* `cons` operand is the place counts; consing the place as the element (`(cons x other)`), a compound place, and a multi-pair `setf` are not flagged. |
| `cons-to-list` | Report a `cons` whose tail is `nil`/`()` or a `list` literal, which is really a `list` construction: `(cons a nil)` is `(list a)`, `(cons a (list b c))` is `(list a b c)`. Auto-fixable (rewrites to `list`; a spelled-out cons chain like `(cons a (cons b nil))` converges to `(list a b)` one layer per fixpoint pass). A cons onto a variable or an improper pair `(cons a b)` is a genuine cons and not flagged. |
| `double-reverse` | Report a `(reverse (reverse x))`; `reverse` returns a fresh sequence of the same kind with the order flipped, so reversing twice yields a fresh sequence equal to `x` — exactly `(copy-seq x)`, a wasteful obfuscated copy. Auto-fixable (rewrites to `(copy-seq x)`, copying the inner argument verbatim). The destructive `nreverse` (on either level) and a mixed `reverse`/`nreverse` nesting are left alone — they cannot be reasoned about as a plain copy. |
| `append-list-to-cons` | Report a two-argument `(append (list x) rest)` whose first argument is a one-element `(list x)`; `append` conses the copied singleton onto the shared tail, which is exactly `(cons x rest)` — same single fresh cons, same sharing of `rest`, same left-to-right evaluation. Auto-fixable (rewrites to `(cons x rest)`). A multi-element first list (`(append (list x y) rest)` is `(list* x y rest)`), a non-`list` first argument, and a different argument count are left alone. |
| `list-star-to-cons` | Report a two-argument `(list* a b)`; by definition `list*` of exactly two arguments builds one cons, so it is exactly `(cons a b)`. Auto-fixable (rewrites to `(cons a b)`). A single-argument `(list* x)` (which is `x`, `single-operand-list-op`'s concern) and a three-or-more-argument `list*` (nested conses) are left alone. |
| `values-list-of-list` | Report a `(values-list (list a b))` whose sole argument is a `list` construction; building a fresh list only to spread it is exactly `(values a b)` — same values, order, and evaluation. Auto-fixable (rewrites to `(values a b)`; an empty `(list)` becomes `(values)`). A quoted list (`'(a b)`, whose elements are data, not forms), a variable argument, and a non-`list` constructor are left alone. |
| `manual-pushnew` | Report a `setf`/`setq` that manually `adjoin`s an element onto a variable (`(setf set (adjoin item set))`, `(setf seen (adjoin k seen :test #'equal))`), which is exactly what `pushnew` expresses. Auto-fixable (rewrites to `(pushnew item set)`, passing any `:test`/`:key` keyword arguments through unchanged). Only a form whose *second* `adjoin` operand is the place counts; a compound place and a multi-pair `setf` are not flagged. |
| `nested-cxr` | Report nested `car`/`cdr`-family accessors that the standard combines into one (`(car (cdr x))` is `(cadr x)`, `(cdr (cdr x))` is `(cddr x)`). Auto-fixable (collapses to the combined `cXr`; deeper nestings converge one layer per fixpoint pass, so `(car (cdr (cdr x)))` becomes `(caddr x)`). Only combinations that stay within the four-letter standard accessors are flagged; `first`/`rest` spellings and a nesting that would exceed `cddddr` are not. |
| `redundant-identity` | Report an `(identity x)` call, which returns its argument unchanged — so it is exactly `x`. Auto-fixable (replaces the form with its argument). Composes with `redundant-funcall` (`(funcall #'identity x)` → `(identity x)` → `x`). A `#'identity` function reference (e.g. a `:key` argument) and an argument-mismatched `(identity)`/`(identity a b)` are not flagged. |
| `nthcdr-zero` | Report `(nthcdr 0 list)`, which returns the list unchanged, so it is just `list`. Auto-fixable (replaces the whole form with the list operand). Only the bare integer `0` count is matched; a non-zero index, a float `0.0`, and a variable count are left alone. |
| `subseq-zero` | Report a two-argument `(subseq seq 0)`; a subseq from index 0 with no end is a fresh copy of the whole sequence, exactly `(copy-seq seq)`. Auto-fixable (rewrites to `(copy-seq seq)`). A present end argument (`(subseq seq 0 n)`, a genuine slice), a non-zero start, a float `0.0`, and a variable start are left alone. |
| `nthcdr-small-index` | Report `(nthcdr n list)` with a literal count `1`–`4`, for which the standard defines a named `cdr` accessor: `(nthcdr 1 x)` is `(cdr x)`, `(nthcdr 2 x)` is `(cddr x)`, `(nthcdr 3 x)` is `(cdddr x)`, `(nthcdr 4 x)` is `(cddddr x)`. Auto-fixable (rewrites to the accessor, copying the list operand). The count `0` belongs to `nthcdr-zero`; `5`+ (no `cdddddr`), a float, and a variable count are left alone. |
| `nth-constant-index` | Report an `nth` with a small literal index that has a named ordinal accessor (`(nth 0 x)` is `(first x)`, … `(nth 9 x)` is `(tenth x)`). Auto-fixable (rewrites to the ordinal). Only literal indices 0–9 are flagged (there is no `eleventh`); a variable index and index 10+ are not. |
| `redundant-apply` | Report an `apply` of a sharp-quoted symbol to a literal list (`(apply #'foo (list a b))`); spreading a literal `(list …)` is exactly the direct call `(foo a b)`. Auto-fixable (rewrites to the direct call; an empty `(list)` yields a zero-argument call). The sibling of `redundant-funcall`. A variable list argument (`(apply #'foo args)`), leading fixed arguments, an ordinary-quoted `'foo`, and a sharp-quoted lambda are not flagged. |
| `sign-comparison` | Report a two-argument `=`/`>`/`<` comparison against the literal `0`, which has a dedicated predicate: `(= x 0)` is `(zerop x)`, `(> x 0)` is `(plusp x)`, `(< x 0)` is `(minusp x)`. Auto-fixable (rewrites to the predicate, accounting for which side the `0` is on — `(> 0 x)` becomes `(minusp x)`). `>=`/`<=`/`/=` (no single-word predicate), a `0.0` spelling, a three-argument comparison, and `(= 0 0)` are not flagged. |
| `negated-comparison` | Report a `not`/`null` wrapping a two-argument numeric comparison, which has an exact complement: `(not (= a b))` is `(/= a b)`, `(not (< a b))` is `(>= a b)`, `(not (> a b))` is `(<= a b)` (and their inverses). Auto-fixable (rewrites to the complementary operator). Only the two-operand shape is flagged — `(not (= a b c))` (all-equal vs pairwise-distinct) and a reader-conditional operand are not. |
| `de-morgan` | Report an `and`/`or` whose operands are *all* single-argument negations, which collapses to one outer negation: `(and (not a) (not b))` is `(not (or a b))`, `(or (not a) (not b))` is `(not (and a b))`. Auto-fixable (rewrites via De Morgan — exact down to short-circuit order). Requires at least two operands, all `not`/`null`; a mix of negated and non-negated operands is not flagged. |
| `redundant-boolean-identity` | Report an `and`/`or` containing its identity element, which contributes nothing: `t` in an `and` (`(and a t b)` is `(and a b)`) or `nil` in an `or` (`(or a nil b)` is `(or a b)`). Auto-fixable (drops the identity operand; collapses to the bare `t`/`nil` if all operands were removed). The complement of `dead-boolean-operand` (which handles the *dominant* `nil`-in-`and`/`t`-in-`or`). A trailing `t` in `and` (its return value) and a single-operand form are not flagged. |
| `constant-if-test` | Report an `if` whose test is the literal constant `t` or `nil`, so one branch is dead code: `(if t a b)` is `a`, `(if nil a b)` is `b`, `(if nil a)` is `nil`. Auto-fixable (`dead-code` category — replaces the form with the live branch). A truthy non-`t` literal (`(if 5 a b)`) and a variable test are not flagged. |
| `constant-when-test` | Report a `when`/`unless` whose test is the literal constant `t` or `nil`, so the body is statically decided: `(when t body…)` and `(unless nil body…)` always run (they are `(progn body…)`), while `(when nil body…)` and `(unless t body…)` never run (they are `nil`, dead code). Auto-fixable (`dead-code` category — splices the always-true form to `progn` and collapses the dead form to `nil`; the discarded body is never evaluated, so no side effects are lost). A truthy non-`t` literal and a variable test are not flagged. The `if` sibling is `constant-if-test`. |
| `negated-if` | Report a three-argument `if` whose test is a `(not X)`/`(null X)` negation (`(if (not ready) a b)`); negating the test just flips the branches, so it is exactly `(if X B A)`. Auto-fixable (drops the negation and swaps the then/else branches). A one-armed `(if (not c) a)` (no else to swap — the `when`/`unless` idiom's job) and a reader-conditional branch are not flagged. |
| `duplicate-setf-places` | Report a `setf`/`setq`/`psetf`/`psetq` that assigns the same variable more than once in one form (`(setf a 1 a 2)`, `(setq x 1 y 2 x 3)`); the earlier assignment is dead — almost always a copy-paste slip or a typo. Report-only (error severity, `duplicate` category). Only symbol places are compared by name (case-insensitively); a compound `setf` place and a malformed odd-arity form are not flagged. |
| `single-operand-boolean` | Report single-operand `and`/`or` forms; a one-argument `and`/`or` returns its operand unchanged, so `(and X)` and `(or X)` are just `X`. |
| `single-arg-comparison` | Report numeric comparisons (`<` `>` `<=` `>=` `=` `/=`) called with a single argument; these are vacuously true (`(< x)` is always `t`), usually a missing operand. |
| `format-missing-destination` | Report `format` calls whose first argument is a string literal; the first argument is the destination (`nil`/`t`/stream), so `(format "~a" x)` is missing it. |
| `format-to-string` | Report a `(format nil "~A" x)` / `(format nil "~S" x)` whose control string is exactly one directive; with a `nil` destination `format` returns the string, and a lone `~A`/`~S` is `princ`/`prin1` semantics, so these are exactly `(princ-to-string x)` / `(prin1-to-string x)`. Auto-fixable (rewrites to the string function). Any surrounding text or extra directive, a non-`nil` destination (`t`/stream return `nil`, not the string), and an argument count other than one are left alone. |
| `format-newline` | Report a `(format t "~%")`; with the `t` destination `format` writes to `*standard-output*` and returns `nil`, and `~%` emits one newline — exactly `(terpri)`. Auto-fixable (rewrites to `(terpri)`). Only the `t` destination is matched (an arbitrary destination could be a fill-pointer string, not a valid `terpri` argument); `~&` (`fresh-line`, whose return value differs) and any call carrying format arguments are left alone. |
| `literal-place` | Report `incf`/`decf`/`push`/`pop`/`pushnew`/`setf`/`psetf` whose place is a self-evaluating literal (`(incf 5)`, `(push x 3)`, `(setf 5 x)`); a literal is not a modifiable place, so the form fails at macroexpansion. |
| `unreachable-cond-clause` | Report cond forms with clauses after a t catch-all clause that can never run. |
| `malformed-let-binding` | Report let/let* bindings that are neither a symbol nor a (var value) pair. |
| `if-arity` | Report if forms with the wrong number of arguments (Common Lisp if takes 2 or 3). |
| `malformed-cond-clause` | Report cond clauses that are not a non-empty list (a bare atom or empty clause). |
| `malformed-case-clause` | Report case/typecase-family clauses that are not a non-empty list (a bare atom or empty clause). |
| `unreachable-case-clause` | Report case/typecase clauses after a t/otherwise catch-all clause that can never run. |
| `malformed-iteration-spec` | Report dolist/dotimes specs that are not a (var form [result]) list. |
| `duplicate-lambda-list-keyword` | Report lambda lists that repeat a lambda-list keyword (&optional, &rest, &key, ...). |
| `lambda-list-keyword-order` | Report lambda lists whose keywords are out of the canonical &optional/&rest/&key/&aux order. |
| `modify-macro-arity` | Report incf/decf/push/pop calls with the wrong number of arguments. |
| `binds-constant` | Report let/let*/do/do* bindings whose variable is a constant (nil, t, or a keyword). |
| `quoted-case-key` | Report case/ecase/ccase clauses with a quoted key ('a matches quote and a, not a). |
| `the-arity` | Report the special forms without exactly two arguments (a type and a form). |
| `equality-arity` | Report eq/eql/equal/equalp calls without exactly two arguments. |
| `accessor-arity` | Report nth/elt/gethash/getf/... accessors with the wrong number of arguments. |
| `setq-non-variable` | Report setq/psetq places that are not variables (a list, literal, or constant). |
| `eval-when-situation` | Report eval-when forms with an invalid situation (not :compile-toplevel/:load-toplevel/:execute). |
| `exhaustive-case-otherwise` | Report ecase/ccase/etypecase/ctypecase forms with a forbidden t/otherwise clause. |
| `duplicate-case-keys` | Report case/ecase/ccase forms with the same key in more than one clause. |
| `self-assignments` | Report setq/setf/psetq/psetf pairs that assign a place to itself. |
| `identical-if-branches` | Report if forms whose then and else branches are structurally identical. |
| `duplicate-cond-tests` | Report cond forms with the same test expression in more than one clause. |
| `duplicate-let-bindings` | Report parallel let forms that bind the same variable more than once. |
| `duplicate-boolean-operands` | Report and/or forms that list the same operand more than once. |
| `eql-string-comparison` | Report eq/eql calls that compare against a string literal (never reliably eql). |
| `self-comparison` | Report comparison calls whose two operands are structurally identical (always true/false). |
| `dead-boolean-operand` | Report and/or forms whose non-final constant operand makes later operands dead. |
| `eq-number-comparison` | Report eq calls that compare against a number literal (eq on numbers is unreliable). |
| `eq-char-comparison` | Report eq calls that compare against a character literal (`(eq c #\a)`); `eq` on characters is unreliable — use `eql` or `char=`. |
| `char-op-string` | Report character functions (`char=`, `char<`, `char-code`, `char-upcase`, `alpha-char-p`, …) applied to a string literal (`(char= "a" c)`); these require a character, so a string literal is a type error. |
| `destructive-literal` | Report destructive calls (`nreverse`, `sort`, `delete`, `nsubstitute`, `nunion`, `nconc`, `rplaca`, …) whose modified argument is a quoted list literal (`(sort '(3 1 2) #'<)`, `(delete x '(1 2))`); modifying a constant is undefined behavior. Each function's sequence argument position is known, so a literal *item* or a last-`nconc` argument is not flagged. |
| `eql-list-comparison` | Report eq/eql calls that compare against a quoted list literal (never reliably eql). |
| `eql-search-literal` | Report `member`/`assoc`/`find`/`position`/`count`/`remove`/`delete`/`adjoin`/`pushnew` (item first) and `substitute`/`nsubstitute`/`subst`/`nsubst` (item second) searching for a string or quoted-list literal with no `:test`; the default `eql` never matches a string/list literal — add `:test #'equal`. |
| `setf-arity` | Report setq/setf/psetq/psetf forms with an odd argument count (a place missing its value). |
| `lint` | Run every within-file logic-bug lint at once and report all findings, tagged by rule and category. Each finding is self-describing — it carries its `severity`, `category`, and a `fixable` flag inline (so an agent can triage and decide whether to run `--fix` without cross-referencing `--list-rules`). `--list-rules` prints the rule catalog with categories, descriptions, a `severity` (`error` for likely/certain bugs, `warning` for redundant/non-idiomatic style), and a `fixable` flag marking the rules `--fix` can repair — and it honors the same `--rule`/`--exclude`/`--category` selectors, so `--list-rules --category dead-code` lists just that group; `--rule`/`--exclude` select rules; `--category` selects a whole group (arity, dead-code, duplicate, malformed, suspicious); `--sarif` emits a SARIF 2.1.0 log for CI code scanning (with stable fingerprints and one-click `fixes` for every rule `--list-rules` marks fixable); `--github` emits GitHub Actions `::error::` annotations for inline PR review; `--fix` applies those auto-fixes in place, iterating to a fixpoint (so nested redundancies collapse fully) and reporting the per-file/per-rule counts; add `--diff` to preview the changes as a unified diff without writing, or `--check` to write nothing and exit 3 when any auto-fix is still pending (a CI gate that stays green only when fixable lint has been cleaned up — distinct from `--fail-on-finding`, which also gates on report-only findings). `--check` and `--diff` combine (show the diff and fail). `--fix-plan` instead emits the machine-readable fix plan — each fixable finding's exact byte-region replacements as JSON (or tab-separated text) — without writing, so an editor or agent can preview or apply fixes one at a time (honoring the same suppressions and `--baseline` as `--fix`). Findings can be silenced in source with an inline `; paredit:ignore [rule…]` comment: on its own line it suppresses the next line, trailing after code it suppresses that line, and with no rule names it suppresses every rule — honored uniformly across the report, SARIF, GitHub, and `--fix` outputs. `--fail-on <error\|warning>` gates only on findings at or above a severity (so CI can block on bugs while still reporting style warnings), and SARIF `level` reflects each finding's severity. `--stats` prints a lint-debt rollup instead of individual findings — finding counts by severity, by category, and by rule, plus files-scanned/files-with-findings — honoring the same `--rule`/`--category`/`--baseline` filters. `--report-unused-suppressions` instead reports any `; paredit:ignore` that silences no finding (a stale ignore or a typo'd rule name) and exits 3 if any are found, keeping the ignore list honest in CI. For adopting the linter on an existing codebase, `--write-baseline <file>` snapshots today's findings and `--baseline <file>` then suppresses those known findings (matched by rule and trimmed-line content, so they survive line shifts) — reporting and gating only on new findings, across the default, `--sarif`, and `--github` outputs. |

Most reports accept `--output json` for machine-readable results.

## Edit

`paredit edit` makes one structural transformation on the form selected by
`--path` or `--at` (see [Selecting forms](selectors.md)). By default the
rewritten document is printed to standard output and the file is untouched.
Mutating commands also accept:

- `--diff` — print a unified diff against the input instead of the whole
  rewritten document.
- `--write` — persist the result back to `--file`. The write is refused if
  the rewritten document no longer parses, and file writes are staged with
  automatic rollback.

| Command | Purpose |
| --- | --- |
| `format` | Print a canonical, indentation-based rendering. |
| `repair-unclosed-lists` | Append matching delimiters for parser-detected unclosed lists; refuse all other parse errors. |
| `select` | Print the S-expression selected by `--path` or `--at`. |
| `replace` | Replace the selected S-expression with replacement text. |
| `kill` | Remove the selected S-expression. |
| `wrap` | Wrap the selected S-expression in a new list. `--delimiter paren\|bracket\|brace` chooses the delimiter (default `paren`). |
| `splice` | Remove one list pair while keeping its children. |
| `split` | Split the enclosing list in two immediately before the selected expression. |
| `join` | Join the selected list with its next sibling list, or two adjacent string literals, into one form. |
| `splice-killing-backward` | Splice the enclosing list, keeping the selection and following siblings while removing preceding ones. |
| `splice-killing-forward` | Splice the enclosing list, keeping the preceding siblings while removing the selection and following ones. |
| `convolute` | Reverse the nesting of the two lists enclosing the selected list. |
| `raise` | Replace the selected expression's parent list with the selection. |
| `transpose-forward` | Exchange the selected expression with its next sibling while keeping trivia in place. |
| `transpose-backward` | Exchange the selected expression with its previous sibling while keeping trivia in place. |
| `slurp-forward` | Pull the next sibling into the selected list. |
| `slurp-backward` | Pull the previous sibling into the selected list. |
| `barf-forward` | Push the last child out of the selected list. |
| `barf-backward` | Push the first child out of the selected list. |

For example, preview then apply a wrap of the third child of the first
top-level form:

```sh
paredit edit wrap --file source.lisp --path 0.2 --diff
paredit edit wrap --file source.lisp --path 0.2 --write
```

## Refactor

`paredit refactor` contains the reviewable workflow commands and the semantic
refactorings they gate. See [Refactor workflow](workflows.md) for the
plan/preview/verify/apply lifecycle.

### Workflow commands

| Command | Purpose |
| --- | --- |
| `plan` | Produce an ordered, gated refactoring plan for AI coding agents. |
| `verify` | Verify pre/post refactoring invariants for agents and CI gates. |
| `preview` | Preview exact refactoring rewrites without modifying files. |
| `check` | Validate a refactor preview manifest without writing files. |
| `status` | Summarize a preview manifest into agent-safe next actions. |
| `apply` | Apply a previously generated preview manifest with hash guards. |
| `diff` | Render a verified diff from a preview manifest without writing files. |
| `workspace-plan` | Discover Lisp sources under roots and build a gated refactor plan. |
| `workspace-preview` | Discover sources and preview exact refactoring rewrites. |
| `workspace-execute` | Execute a workspace refactor with preview gates and post-write verification. |

### Definition and file layout

| Command | Purpose |
| --- | --- |
| `remove-definition` | Plan or remove a top-level definition from one file. |
| `remove-unused-definitions` | Plan or remove unused top-level definitions across files. |
| `move-definition` | Plan or move a top-level definition between files. |
| `split-file` | Plan or split multiple top-level definitions into another file. |
| `sort-definitions` | Plan or sort contiguous top-level definition blocks in one file. |
| `move-form` | Plan or move any top-level form between files. |
| `insert-top-level` | Insert exactly one top-level S-expression before, after, or at the end of a file. |
| `replacement-plan` | Convert duplicate groups into reviewed replace-forms batches. |
| `replace-forms` | Plan or replace multiple reviewed forms in one file. |

### Packages

| Command | Purpose |
| --- | --- |
| `add-export` | Plan or add a symbol to a Common Lisp `defpackage` `:export` option. |
| `sort-package-exports` | Plan or sort `defpackage` `:export` symbol designators. |
| `sort-package-options` | Plan or sort `defpackage` option forms. |
| `merge-package-options` | Plan or merge duplicate `defpackage` option forms. |
| `rename-package` | Plan or rename package designators and qualified prefixes. |

### Renames

| Command | Purpose |
| --- | --- |
| `rename-at` | Rename whatever symbol occupies a byte offset, dispatching to the owning namespace and scope. |
| `rename-symbol` | Rename exact atom occurrences without touching strings or comments. |
| `rename-in-form` | Rename exact atom occurrences inside one selected form. |
| `rename-binding` | Rename one local binding and only the references in its lexical scope. |
| `rename-symbols` | Plan or apply an exact atom rename across explicit files. |
| `rename-function` | Plan or apply a Common Lisp callable definition and designator rename. |
| `rename-macrolet` | Plan or apply a `macrolet`/`compiler-macrolet` binding and call-site rename. |
| `rename-symbol-macro` | Plan or apply a `define-symbol-macro` binding and value-reference rename. |
| `rename-local-function` | Plan or apply a `flet`/`labels` local function binding and call-site rename. |

### Calls and functions

| Command | Purpose |
| --- | --- |
| `replace-function-calls` | Plan or replace callable call-site heads across explicit files. |
| `wrap-function-calls` | Plan or wrap callable call sites in another function or macro call. |
| `unwrap-function-calls` | Plan or remove a unary wrapper around callable call sites. |
| `unwrap-call` | Replace one selected wrapper call with one selected argument. |
| `thread-expression` | Convert a nested call chain into a thread-first or thread-last pipeline. |
| `unthread-expression` | Convert a threading pipeline back into nested calls. |
| `extract-function` | Extract the selected expression into a top-level function with inferred parameters. |
| `extract-local-function` | Extract the selected expression into a Common Lisp `flet` or `labels` binding. |
| `extract-constant` | Extract the selected expression into a top-level constant. |
| `inline-function` | Inline one selected function call using a selected function definition. |
| `inline-lambda` | Replace a safe, immediately invoked Common Lisp lambda with a parallel `let`. |
| `inline-local-function` | Inline the sole direct call in a safe, single-binding Common Lisp `flet` form. |
| `inline-symbol-macro` | Expand a conservative single-binding Common Lisp `symbol-macrolet` form. |
| `inline-literal-constant` | Inline an immutable self-evaluating Common Lisp `defconstant` value. |
| `convert-labels-to-flet` | Convert a non-recursive Common Lisp `labels` form into `flet`. |
| `convert-flet-to-labels` | Convert a Common Lisp `flet` form into `labels` when definition references cannot be captured. |
| `rename-block` | Rename a selected Common Lisp `block` and matching `return-from` references. |
| `rename-tag` | Rename one tag in a selected Common Lisp `tagbody` and matching `go` references. |
| `remove-unused-block` | Remove a selected Common Lisp `block` with no matching `return-from`. |
| `remove-unused-tag` | Remove an unreferenced tag from a selected Common Lisp `tagbody`. |

### Parameters and bindings

| Command | Purpose |
| --- | --- |
| `add-function-parameter` | Add a parameter to a selected function and explicit call sites. |
| `move-function-parameter` | Move one positional parameter in a function and its call sites. |
| `swap-function-parameters` | Swap two positional parameters in a function and its call sites. |
| `reorder-function-parameters` | Reorder all positional parameters in a function and its call sites. |
| `remove-function-parameter` | Remove one positional parameter from a function and its call sites. |
| `introduce-let` | Replace the selected expression with a local binding in the enclosing list. |
| `inline-let` | Inline a single local let binding into its body. |
| `convert-let-to-let-star` | Convert a Common Lisp or Emacs Lisp `let` to `let*` when later initializers do not reference earlier bindings. |
| `convert-let-star-to-let` | Convert a Common Lisp `let*` to `let` when later initializers do not reference earlier bindings. |
| `convert-do-star-to-do` | Convert a Common Lisp `do*` to `do` when later initializers and step expressions do not reference earlier bindings. |
| `convert-prog-star-to-prog` | Convert a Common Lisp `prog*` to `prog` when later initializers do not reference earlier bindings. |
| `merge-nested-let-star` | Merge a directly nested Common Lisp or Emacs Lisp `let*` into one sequential binding form. |
| `split-let-star` | Split a Common Lisp or Emacs Lisp `let*` into nested sequential binding forms at `--binding-index`. |
| `merge-nested-let` | Merge directly nested Common Lisp or Emacs Lisp parallel `let` forms when inner initializers are independent. |
| `merge-nested-flet` | Merge directly nested Common Lisp `flet` forms when inner definitions do not reference outer local functions. |
| `split-let` | Split a Common Lisp or Emacs Lisp parallel `let` at `--binding-index` without capturing initializer references. |
| `eliminate-empty-binding-form` | Remove an empty Common Lisp or Emacs Lisp `let` or `let*` from a known expression position. |
| `flatten-progn` | Flatten directly nested Common Lisp or Emacs Lisp `progn` forms in a safe expression context. |
| `convert-if-to-cond` | Convert a Common Lisp or Emacs Lisp `(if test then [else])` form to `cond`. |
| `convert-cond-to-if` | Convert simple Common Lisp or Emacs Lisp `cond` clauses to nested `if` forms. |
| `convert-when-to-if` | Convert a Common Lisp or Emacs Lisp `when` form to `if`. |
| `convert-unless-to-if` | Convert a Common Lisp or Emacs Lisp `unless` form to `if`. |
| `convert-if-to-when` | Convert a Common Lisp or Emacs Lisp `if` without a meaningful else to `when`. |
| `convert-if-to-unless` | Convert a Common Lisp or Emacs Lisp `if` with a literal `nil` then branch to `unless`. |
| `remove-unused-binding` | Plan or remove one unused local let binding. |
