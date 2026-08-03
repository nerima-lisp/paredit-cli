# paredit-feature-lint-convention

Lint rules about what a definition *says about itself*: whether its name obeys
the convention its own defining form implies, whether it documents what it
takes, whether its declarations agree with its lambda list, and whether its
CLOS options are ones CLOS has.

Three of the four categories here (`naming`, `documentation`, and the softer
half of `object-system`) are tagged `pedantic`, which means `--preset
recommended` leaves them out. That is the honest placement: a project that has
not adopted `+constant+` is not making a mistake by not adopting it, and a rule
that fires on every definition in such a project is noise, not information.

The `declaration` rules are not pedantic and not warnings. `(declare (ignore
x))` on a variable the body goes on to use is a compile-time error in most
implementations and a latent one in the rest.

## What the corpus audit changed in `ignore-declaration-conflict`

Run over **1291 third-party Common Lisp files** (SBCL 2.6.0's own `src/` and the
installed Quicklisp distribution, 25.7 MB, containing 7690 `(declare`, **2224
`(ignore`** and 424 `(ignorable` occurrences across 21945 `defun`/`defmacro`/
`defmethod`/`lambda` heads), the shipped rule produced **45 findings**. Two were
real. The other 43 were false positives on code SBCL compiles clean, in six
classes — and the two largest were not about the body walk at all but about the
rule never having learned to read a lambda list:

| Class | Findings | What was wrong |
|---|---|---|
| Destructuring macro lambda lists | 21 | A sublist in required position was read as `(name specialiser)`, so every name in `(defmacro m ((a b) …))` past the first was "not a parameter". |
| `supplied-p` variables | 5 | `(name default supplied-p)` was read as naming only `name`, so `(&optional (o 1 op))` left `op` unbound. |
| Quoted and templated declarations | 8 | The walk was unfiltered, so `` `(lambda ,args (declare (ignore ,@dummies)) …) `` reported a variable literally spelled `,@dummies`, and a symbol appearing inside a backquote counted as a use. |
| Shadowing | 8 | No rebinding check, so an inner `lambda` — or even its *parameter list* — counted as a use of the outer binding. |
| Lisp-2 operator position | 1 | `(signal int)` names a function; the parameter `signal` is a different namespace. |

The two survivors are both `(defun %thread-yield () (declare (ignore thread)))`
in `bordeaux-threads`' `impl-corman.lisp` — a zero-argument definition carrying a
declaration copy-pasted from its neighbour, which is exactly the mistake the
`NotBound` half of the rule exists to catch. After the fix the same corpus
reports those two and nothing else.

**The failure mode of every guard below is silence**, so each is paired in
`src/ignore_declaration_conflict.rs` with a control that must still fire and
differs only in the detail the guard keys on, and each was mutation-tested by
removing it and confirming exactly which controls then fail. Four controls did
*not* fail on the first attempt — they rebound a different name than the one
under test, so the shadowing branch was never entered — and were rewritten.

The rule's own lint golden is not evidence here: `tests/fixtures/lint_golden`
contains no `(declare (ignore …))` at all, so this rule is pinned at zero
findings in all four goldens both before and after. The corpus differential is
what carries the claim.

### Deliberately not modelled

* **`flet`/`labels`/`macrolet` clause shadowing.** A clause whose lambda list
  rebinds the name is still walked, so such a use is still counted. That errs
  towards reporting.
* **Whether an unknown macro evaluates its argument.** Undecidable at file
  scope. The `with-…` binder table is curated rather than a `with-` prefix test
  precisely because `(with-simple-restart (continue "…") …)` has the identical
  shape and binds nothing.
