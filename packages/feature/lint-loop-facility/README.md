# paredit-feature-lint-loop-facility

Lint rules for the Common Lisp `loop` macro's own clause grammar.

`loop` is the one standard macro with a grammar of its own rather than an
S-expression shape (CLHS 6.1), and the defects it admits are *relational*: they
live in how two clauses relate, not in the shape of any one form. That is the
premise of this crate, and it is why these rules could not be written as
ordinary shape checks.

Every premise below was settled by running SBCL 2.6.0, not by reasoning about
the standard. Several plausible rules died that way; see "Rules that were
proposed and dropped".

| rule | category | severity | fix | heads |
| --- | --- | --- | --- | --- |
| `loop-parallel-binding-reads-sibling` | suspicious | error | report-only | `loop` |
| `loop-into-accumulator-never-read` | dead-code | error | report-only | `loop` |
| `loop-accumulation-discarded-by-finally-return` | dead-code | warning | report-only | `loop` |

All three are Common Lisp only, which is this workspace's default rule dialect
scope, and all three are reached only through the `loop` head.

## The three rules

### `loop-parallel-binding-reads-sibling`

Successive `for`/`as`/`with` clauses bind sequentially, like `let*`. Clauses
joined by `and` bind *simultaneously*, like `let` (CLHS 6.1.1.4). So a clause's
initial value cannot read a variable bound in the same `and` group. The two
spellings differ by three characters and give different answers:

```lisp
(loop for a from 1 to 3 and b = (* a 10) collect (list a b))
;; => ((1 10) (2 10) (3 20))   ; SBCL 2.6.0, no warning of any kind
(loop for a from 1 to 3 for b = (* a 10) collect (list a b))
;; => ((1 10) (2 20) (3 30))   ; what the author meant
```

The same shape also produces hard errors — `TYPE-ERROR: Value of A in (* A 10)
is NIL` for a `for … = … then …` sibling, and `UNBOUND-VARIABLE` for a `with`
one — but the silent case above is the one that earns the rule.

**The `then` exclusion is what keeps this rule honest.** A sibling read in a
`then` *step* form is not a defect; it is the standard "previous element"
idiom, and it *requires* `and`:

```lisp
(loop for x in '(1 2 3) and prev = nil then x collect (cons prev x))
;; => ((NIL . 1) (1 . 2) (2 . 3))   ; correct
(loop for x in '(1 2 3) for prev = nil then x collect (cons prev x))
;; => ((NIL . 1) (2 . 2) (3 . 3))   ; the sequential spelling is the wrong one
```

So the rule reports a read from an **init** position only — the `=` init form
and the `in`/`on`/`across`/`from`/`to`/`by` operands, every one of which is
evaluated once at loop setup, when the sibling still holds `nil` or is not
bound at all.

### `loop-into-accumulator-never-read`

Naming an `into` variable takes the accumulation out of the loop's implicit
result (CLHS 6.1.3), and the variable's scope ends with the loop. If nothing
reads it, the loop returns `nil` and the accumulated value is unreachable.

```lisp
(loop for x in '(1 2 3) collect x into acc)   ; => NIL  (+ a style warning)
(loop for x in '(1 2 3) sum x into total)     ; => NIL  (no warning at all)
```

The second line is why the rule earns its place. For a list accumulator SBCL
can see the variable is never read. For a *numeric* one it structurally cannot:
`sum … into total` expands to `total = total + x`, so `total` **is** read, by
its own accumulation, and the compiler has nothing to report.

### `loop-accumulation-discarded-by-finally-return`

An extended `loop` has at most one implicit result, from the accumulation
clauses that name no `into` (CLHS 6.1.1.3). A `finally (return …)` pre-empts it
(CLHS 6.1.2.3) — and because an implicit accumulation has no name, the
`finally` can never be returning it.

```lisp
(loop for x in '(1 2 3) collect x finally (return :other))   ; => :OTHER
```

The list is fully consed on the way there and read by nothing.

## Rules that were proposed and dropped

Seven rules were proposed for this batch. Four were dropped on evidence, and
the reasons are recorded because each is a trap a later batch could walk into
again.

- **`loop-with-clause-after-for`** — refuted twice over. SBCL compiles and runs
  `(loop for x in '(1 2 3) with total = 0 collect (+ x total))` clean, with no
  warning; both are variable clauses and CLHS 6.1.1.1.1 admits them in either
  order. `lint-iteration-flow`'s `loop-clause-order-violation` already
  documents this exact idea as the one ordering complaint that would be wrong,
  and carries a test asserting silence on it.
- **`loop-accumulates-into-two-kinds`** (implicit, no `into`) — SBCL rejects it
  at **macroexpansion time** with a hard `ERROR`: "incompatible kinds of LOOP
  value accumulation specified for collecting as the value of the LOOP: LIST
  and SB-LOOP::SUM". A rule cannot beat a compile-time error. The same shape is
  also already reported by `lisp-analysis`'s `inspect loop`
  `conflicting-accumulation` finding.
- **`loop-for-on-with-non-list-step`** — `(loop for x on '(1 2 3) by #'cddr
  collect (car x))` returns `(1 3)`: stepping by `cddr` is *the* idiomatic
  plist walk, not a defect, and "the body assumes single stepping" is not
  decidable from the clause grammar.
- **`loop-conditional-clause-without-else-accumulation`** — a `when p collect x`
  with no `else` is ordinary, correct Common Lisp and usually exactly what the
  author meant. There was no defect to detect, only a shape. Dropped without
  implementation rather than shipped as `pedantic` on plausibility.

Two more were reshaped rather than dropped:

- **`loop-finally-return-shadowed-by-accumulation`** narrowed into
  `loop-accumulation-discarded-by-finally-return`. As originally stated it
  overlapped `lint-form-shape`'s `loop-collect-into-immediately-returned`; the
  residue that rule cannot reach — an *implicit* accumulation discarded by a
  `finally` returning something else — is what shipped.
- **`loop-destructuring-arity-mismatch`** is real (`(loop for (a b) in
  '((1 2 3) (4 5 6)) …)` silently drops the third element, and `(loop for
  (a b c) in '((1 2) …) …)` silently yields `NIL`) but is decidable only when
  both the pattern and the list are literal. The corpus sweep found no such
  occurrence, so shipping it would have meant a rule with a zero denominator —
  a guaranteed false clean. Recorded here rather than implemented.

## Third-party corpus audit

Swept over SBCL 2.6.0's own sources and contribs, the local Quicklisp dist, and
the 243 Common Lisp packages in the Nix store, dispatching through the real
engine. The harness was self-tested on a known-dirty file first — a sweep that
reports zero because the harness is broken is a false clean, not a result.

| | files | `loop` forms | modelled |
| --- | --- | --- | --- |
| SBCL src + contrib + Quicklisp | 1588 (31 unparsed) | 2733 | 2082 |
| Nix-store CL packages | 12606 (59 unparsed) | 20604 | 16598 |

Candidates and findings over the larger sweep:

| rule | candidates | findings |
| --- | --- | --- |
| `loop-parallel-binding-reads-sibling` | 132 parallel `and` groups | 0 |
| `loop-into-accumulator-never-read` | 772 `into` clauses | 0 |
| `loop-accumulation-discarded-by-finally-return` | 5627 implicit accumulations | 0 |

**The audit changed the code.** Before it,
`loop-into-accumulator-never-read` reported **41 findings over SBCL's own
sources, every one of them a false positive** — all the same shape:

```lisp
(loop for i in funs collect `(define-alien-routine ,i void) into defines
      finally (return `(progn (declaim (inline ,@funs)) ,@defines)))
```

`defines` *is* read, by the `,@defines`. The occurrence counter bailed on any
node carrying a reader prefix and so never descended into the `finally`'s
template. Replacing it with the two-counter quote model
(`shared::count_evaluated_reads`) fixed all 41. The counter still refuses a
mention under a hard `'`, and a symmetric negative test pins that, so the fix
did not over-correct into silence.

## Cost

Measured with `PassOptions { measure: true }` at two file sizes, in the same
pass as the shipped `loop-clause-order-violation` so both share every source of
run-to-run noise. Absolute nanoseconds swing between sessions under load
(`uptime` reported a load average near 84); the ratios are the result.

| rule | ns/call (113 KB) | ns/call (226 KB) | doubling |
| --- | --- | --- | --- |
| `loop-parallel-binding-reads-sibling` | 1960 | 2069 | 2.11x |
| `loop-into-accumulator-never-read` | 3268 | 3453 | 2.11x |
| `loop-accumulation-discarded-by-finally-return` | 1629 | 1701 | 2.09x |
| `loop-clause-order-violation` (shipped baseline) | 1661 | 1758 | 2.12x |

All three scale linearly and sit within 2x of a shipped rule on the same head.

**The guard ordering is what buys that**, and it was verified by breaking it.
Every rule runs its cheap, form-local grammar check first and reaches
`is_unevaluated_at` — which descends from `root_view` and so materializes the
whole document — only with a finding otherwise ready. Inverting that order in
all three rules and re-measuring:

| rule | ns/call (113 KB) | ns/call (226 KB) | doubling |
| --- | --- | --- | --- |
| `loop-parallel-binding-reads-sibling` | 820568 | 1721327 | 4.20x |
| `loop-into-accumulator-never-read` | 830261 | 1731164 | 4.17x |
| `loop-accumulation-discarded-by-finally-return` | 813624 | 1727714 | 4.25x |

A 250-500x per-call regression, and the doubling ratio goes from linear to
**quadratic** — the per-call cost is itself linear in file size, and there are
linearly many calls. This is the same inversion a sibling batch measured at
450843 ns/call against 28.

## Relationship to `lint-iteration-flow`

That package's `loop_syntax.rs` solves the same tokenizing problem, and this
crate's `loop_grammar.rs` reproduces its three guards — bound names are never
keywords, operand positions are never keywords, an unmodelled sub-grammar
aborts the whole form. Those were arrived at by corpus measurement and are not
reinvented lightly.

The duplication is deliberate for two reasons. A feature-to-feature dependency
needs an entry in `tests/cli/feature_dependency_contract.rs`, which this batch
was not permitted to edit. More importantly the existing reader could not
answer this crate's question anyway: it documents compound `and` clauses as
"tokenized but never interpreted", and lists `and` among its `NAME_INTRODUCERS`
specifically so that it over-collects and *loses* findings. Parallel-group
structure is exactly what `loop-parallel-binding-reads-sibling` needs.

**The merge is worth doing and belongs in its own pass.** `loop_grammar.rs`'s
tokenizer is a superset of `loop_syntax.rs`'s: it adds `ParallelGroup`,
init-versus-step operand roles, and `of-type` skipping, and it drops the
`LoopScan`/`FileFindings` plumbing the standalone `inspect` commands need. The
right home for the merged reader is `packages/core/`, which this batch was also
not permitted to modify — so it is reported as a gap rather than attempted.

## Gaps found and not fixed

- **`lisp-analysis`'s `loop_report` mishandles `into` for extremum verbs.** Its
  `EXTREMUM_ACCUMULATORS` path steps `index += 2` with no `into` handling, so
  `(loop for x in xs collect x maximize x into m)` is reported as a
  `conflicting-accumulation` even though `into m` separates the two. That is a
  false positive in the existing implicit-case implementation, in a package
  this batch did not touch.
- **`loop_report`'s `unterminated` finding is inspect-only.** A `loop` whose
  only `for` clauses are the non-terminating `= form [then form]` kind, with no
  `while`/`until`/`repeat`/`return`, cannot terminate — SBCL macroexpands
  `(loop for x = 1 collect x)` to a `TAGBODY` whose only transfer is an
  unconditional `(GO NEXT-LOOP)`, leaving `END-LOOP` as unreachable dead code.
  The detection already exists as an `inspect loop` finding, so no lint rule
  was added here; that it is unreachable from the lint suite is the gap.
