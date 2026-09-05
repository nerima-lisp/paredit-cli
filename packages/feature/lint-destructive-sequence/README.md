# paredit-feature-lint-destructive-sequence

One lint rule for Common Lisp's **destructive sequence functions**: a call whose
return value is discarded, on a variable the same body goes on to read.

CLHS 17.1 says such a function's argument "may be destroyed and used to
construct the result", and that the consequences are undefined if the original
is used afterwards. Only the return value names the finished sequence.

Status: **unregistered**. The rule is complete, tested and audited, but is not
wired into the root registry.

## The rule

| Rule | Category | Severity | Fixability |
|---|---|---|---|
| `discarded-destructive-sequence-result` | `DeadCode` | `Warning` | `ReportOnly` |

`RuleDialectScope::COMMON_LISP_ONLY`, stated explicitly rather than inherited
from the trait default. `HeadFilter::Heads`; not `WholeTree`.

**Heads** (20) — the implicit-progn operators the rule *anchors* on, not the
destructive functions it reports:

```text
progn  prog  prog*  let  let*  flet  labels  macrolet  symbol-macrolet
lambda  when  unless  dolist  dotimes  block  with-open-file  with-slots
defun  defmethod  defmacro
```

**Reported operators** (6) — `sort`, `stable-sort`, `nconc`, `nbutlast`,
`nsublis`, `nsubst`.

### Why the anchor is the body and not the call

"Discarded" is a property of a form's *position*, so deciding it needs the
parent — and `RuleContext` carries no parent pointer. Recovering one means
descending from `SyntaxTree::root_view()`, which **materializes the whole tree**:
one `ExpressionView` per node, each with its own `Vec`s, O(file) with
allocations per call.

The first version of this rule anchored on `sort`/`nconc`/… and walked up. It
measured **3.9 seconds** on a 200-function fixture with **zero findings**,
against a shipped control's 224 µs in the same run — quadratic, because the
*correct* idiom `(setf xs (sort xs #'<))` passes any cheap head-and-argument
test, so every correct call in the file paid a full materialization. Only the
parent tells the correct idiom from the bug, and reaching the parent is the
expensive thing.

Anchoring on the body form inverts the direction: the dispatcher hands over the
`defun`/`let`/`when`, and the parent-child relation the rule needs is that node's
own children. No tree access, no allocation, linear per file.
`paredit-feature-lint-performance`'s `unnecessary-sort-before-extremum-extraction`
chose the same inversion for the same reason. `cost_tests.rs` keeps the rejected
shape as `cost-control-wrong-order` so the number stays reproducible.

## Why the head list is six long and not twenty-two

**SBCL already catches most of this family.** Every one of the 22 CLHS
destructive sequence functions was compiled with its result discarded and the
compiler's own output recorded (SBCL 2.6.0):

- **Eleven warn**, unconditionally — `nreverse`, `nreconc`, `delete`,
  `delete-if`, `delete-if-not`, `delete-duplicates`, `nunion`, `nintersection`,
  `nset-difference`, `nset-exclusive-or`, `merge` — each with
  `STYLE-WARNING: The return value of X should not be discarded.` and
  `warnings-p=T`. Repeating a diagnostic the compiler gives for free would add
  only noise, so none of them is a head here.
- **`sort` and `stable-sort` are silent, but only sometimes.** SBCL warns once
  it can *prove* the sequence is a list, because it then picks the
  `STABLE-SORT-LIST` transform, which carries the declaration. An untyped
  parameter — the common case — gets nothing:

  ```text
  (defun f (xs) (sort xs #'<) xs)                         => warn=NIL  silent
  (defun f (xs) (declare (list xs)) (sort xs #'<) xs)     => warn=T    STYLE-WARNING
  (defun f (xs) (declare (vector xs)) (sort xs #'<) xs)   => warn=NIL  silent
  ```

- **Five are silent and harmless.** `nstring-downcase`, `nstring-upcase`,
  `nstring-capitalize`, `replace` and `nsubstitute` rewrite elements in place and
  never return a different object — `(let ((s (copy-seq "hello"))) (nstring-upcase s) s)`
  is `"HELLO"` — so reporting them would be a pure false positive.

That leaves six heads that are both silent under SBCL and able to return an
object that is not the argument.

## What the aftermath actually looks like

Recorded rather than assumed, because the folklore ("the variable ends up
holding the last cons") is wrong:

```text
(let ((xs (list 3 1 2)))     (sort xs #'<) xs) => (1 2 3)   accidentally right
(let ((xs (list 5 4 3 2 1))) (sort xs #'<) xs) => (4 5)     two-element interior tail
(let ((xs (list 1 2 3)))     (nreverse xs) xs) => (1)       the *first* cons
```

The first line is why the bug survives review: on a short list it often looks
like it worked.

## The position analysis

A finding needs all three of:

1. **The destroyed argument is a bare symbol.** A literal — `(nreverse '(1 2 3))`
   — is `paredit-feature-lint-sequence`'s **already-shipped**
   `destructive-literal`, which this package deliberately does not duplicate. A
   nested call destroys a temporary nobody can observe.
2. **The value is discarded**: a non-final form in the body of a known
   implicit-progn operator (`support::BODY_FORMS`).
3. **A later form in the same body reads that symbol.** Without this the call is
   merely dead, which has a large innocent population.

| position | verdict |
|---|---|
| non-final form of `progn`/`let`/`let*`/`defun`/`defmethod`/`defmacro`/`lambda`/`when`/`unless`/`dolist`/`dotimes`/`block`/`flet`/`labels`/`macrolet`/`symbol-macrolet`/`prog`/`prog*`/`with-open-file`/`with-slots` | **discarded** |
| last child of any form; any argument of a plain call, including `setf`, `push`, `return-from`, a `let` binding | **used** |
| `tagbody`, `loop` clauses, `cond`/`case` clause bodies, `unwind-protect` cleanups, `prog1`/`prog2` | **ambiguous — never reported** |

`(setf xs (sort xs #'<))` is unreportable structurally, not by a special case:
`setf` is not an implicit-progn operator, so no child of it is ever a discarded
statement.

## Known limitation

`sort` on a **vector** is in-place in SBCL — `(let ((v (vector 5 4 3 2 1))) (sort v #'<) v)`
is `#(1 2 3 4 5)` — so a vector-valued variable is a false positive this rule
cannot rule out without type inference. It is still undefined behaviour under
CLHS and unportable, but it is not a bug that bites on SBCL.

## Verification

51 tests, all passing; `cargo build`, `cargo test`, `cargo clippy --all-targets
-- -D warnings` and `cargo fmt --check` all exit 0.

### The third-party audit — `corpus_audit.rs`

SBCL 2.6.0's own sources and contribs plus the installed Quicklisp dist:

```text
files scanned  : 1619   (898 SBCL + 721 Quicklisp)
files unparsed :   31   (~63 destructive operators, named in the output)
bytes          : 28,378,755

body forms dispatched to the rule       : 56731
destructive calls present (population)  :   295   (218 SBCL + 77 Quicklisp)
  of those, on a bare variable  (cond 1):   122
  of those, in a discarded slot (cond 2):     1
  of those, read by a later form(cond 3):     0   <- FINDINGS
```

**Zero findings over 295 destructive calls**, and the funnel says which condition
did the cutting: condition 2, by a distance. Of 122 destructive calls on a bare
variable, exactly **one** sits in a value-discarding position. Mature Common Lisp
essentially always binds the result.

The one near miss, `sbcl/src/code/globals.lisp:70`, is
`(nconc list (list (list symbol initform)))` — **correct code, correctly
declined**. `list` there is a header cons (the surrounding function reads
`(cdr list)`), so it is non-empty by construction, `nconc` mutates it in place
and returns that same object, and discarding the result is deliberate. Condition
3 declined it because no later form in that body reads `list`. That is the only
evidence in the corpus about whether condition 3 earns its place, and it says
yes.

The zero is believable because the harness self-test passes in the same run
(2/2 planted defects found) and because
`corpus_audit_finds_a_defect_planted_in_a_real_file` splices a defect into a real
corpus file — alexandria's `control-flow.lisp` — and requires the sweep to find
exactly that one while the untouched file reports none.

### Cost — `cost_tests.rs`

Release build, load average 3.32. `n` counts generated functions; the 8x column
is the n=250 → n=2000 ratio, where linear is ~8.

```text
== clean: correct code, every head present, ZERO findings ==
  cost-control-wrong-order        8x ratio = 46  [744907050 … 34465625007] ns
  discarded-destructive-…-result  8x ratio = 10  [   229175 …     2368243] ns
  cost-control-shipped-local      8x ratio =  7  [    31543 …      244800] ns
  cost-control-noop               8x ratio =  8  [    23084 …      189195] ns
```

**3,250x** at n=250, rising to 14,553x at n=2000 — the factor grows because the
shapes differ: the shipped rule is linear, the rejected one quadratic. The rule
sits within 9.7x of a shipped local control.

On a fixture where *every* call is a finding the rule is itself quadratic
(5.37 s at n=2000), because `is_unevaluated_at` materializes the tree once per
reporting body form. That is recorded in `cost_tests.rs` rather than hidden: it
is invisible on correct code, findings are rare (0 in 1619 files), and the whole
28 MB audit sweeps in 0.69 s.

### Mutation testing

All **16 guards** removed or inverted one at a time, rebuilt, run, restored —
and every mutation verified to actually change the file first, since a regex that
does not match reports a false "survived". All 16 are killed by a named test.
Four rounds were needed:

- **G2, G3, G4, G9b** survived the first round as **live guards with missing
  tests**, and each gained one: an atom literal as the destroyed argument, a
  quoted statement inside an evaluated body, a body form inside quoted data
  (which needed an *engine-level* test — the unit helper filters data before the
  rule sees it, so it could never reach the suppression), and a character
  literal / reader conditional for the `#` exclusion.
- **Two guards were genuinely dead** and were removed with the reasoning
  recorded where it is reachable: the `is_bare_symbol` call in
  `subtree_mentions` (unreachable, because `atom_text` carries the reader prefix
  so no excluded shape could compare equal anyway) and `discarded_range`'s
  `start < last` check (an empty range is already inert). Both now have tests
  pinning the behaviour that replaced them.
- `value_is_discarded` was itself found dead — a second spelling of a predicate
  the rule inlined — and is now *defined in terms of* `discarded_range`, so there
  is one implementation and its tests exercise the live one.

## Note on `support.rs`

`support.rs` follows the two-counter `QuoteState` quote model in
`paredit-feature-lint-condition-system::support`. A single `i32` depth counter
is **not** an acceptable substitute because it cannot distinguish hard quotes
from nested quasiquotes.
