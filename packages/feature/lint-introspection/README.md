# paredit-feature-lint-introspection

Lint rules for runtime code generation and introspection.

## Responsibilities

Three rules about programs that build names, definitions, or lookups at run
time instead of writing them down.

| Rule | Flags |
| --- | --- |
| `intern-dynamic-package-target` | an `intern` whose *package* argument is a computed call, so the symbol's destination is not statically knowable |
| `symbol-function-fset-dynamic-name` | a function definition installed under a name built by `intern`, so no search finds the definition |
| `introspection-probe-unchecked` | a probe that answers "not found" with `nil`, applied directly by `funcall`/`apply` with no opportunity to check it |

Every rule is `Fixability::ReportOnly`. The remedy in each case is a design
decision — a literal designator, a dispatch table, a checked branch — and none
of the three is a rewrite a tool may choose.

Every rule declares `HeadFilter::Heads`. Two anchor on the mutating operator
(`intern`; `fset`/`defalias`/`setf`), and the third
anchors on the *consumer* (`funcall`/`apply`) rather than on the probe, because
"was this value checked?" is a question about where the value goes, and a rule
anchored on the probe would have to look upward at context a per-node predicate
does not have.

## Quote handling is the load-bearing part

Every head here — `intern`, `setf`, `funcall`, `fset` — is a spelling that macro
*bodies* are full of, and a macro's output is a backquoted template. The lint
engine's dispatch walks into quoted data like any other subtree, so without
`support::is_unevaluated_at` all three rules would fire on template text:

```lisp
(defmacro define-handler (name)
  `(setf (symbol-function (intern (format nil "~A-handler" ,name)))
         (lambda () ...)))
```

Nothing here is a call. `support.rs` models `'` and `` ` `` as two separate
counters, not one depth number, because a comma inside `'(…)` is a comma
character in a literal list while a comma inside `` `(…) `` escapes back to code;
and it reads the verdict *at* the target rather than at an ancestor, because a
node one level inside a quote carries no reader prefix of its own and is still
data.

## Cost

No rule calls `SyntaxTree::root_view`, `binding_table()`, `value_table()`,
`type_table()`, or `RuleContext::scratch_cache`. No rule correlates separate
top-level forms, so nothing here is quadratic in the number of definitions.

`is_unevaluated_at` is the only part that touches the tree, it runs *last* —
after the whole structural shape has already matched — and it reaches its target
through the one top-level form containing it, located by binary search over
`root_children`. In the `clean/forms/*` benchmarks, which lint files with zero
findings, it never runs at all.

## Dialects

`RuleDialectScope` is stated per rule, not per package, and each spelling was
checked against the dialect's own reference rather than recalled.

| Rule | Dialects | Why not the others |
| --- | --- | --- |
| `intern-dynamic-package-target` | Common Lisp | Emacs Lisp's `intern` takes an *obarray*, and a computed obarray is ordinary Emacs Lisp. Clojure's `(intern ns name val)` takes its namespace first *and* creates a Var binding — it is closer to `def` than to CLHS `intern`. |
| `symbol-function-fset-dynamic-name` | Common Lisp, Emacs Lisp | `fset` and `defalias` are Emacs Lisp functions with no CLHS counterpart; `fdefinition` is CLHS with no Emacs Lisp counterpart. Each spelling is offered only to the dialect that has it. |
| `introspection-probe-unchecked` | Common Lisp, Emacs Lisp, Clojure | Only probes whose reference text says the not-found answer is `nil`. Clojure gets `apply` but **not** `funcall`, which it does not have. |

## What this package does not own

- **No registry.** `REGISTRY` stays in the root and names each rule's `META`
  and `RULE` across this boundary.
- **No engine.** The single pass, head index and rule trait are
  `paredit-core-lint-engine`'s.
- **No `eval-of-macroexpanded-form` rule.** Asked for, and deliberately **not
  implemented**: `paredit-feature-lint-safety`'s `eval-of-non-constant` anchors
  on `eval` and reports whenever the argument is not a literal the source shows.
  `(eval (macroexpand form))` passes a `(…)` call, which is never a literal, so
  that rule already fires on *every* instance — the proposed trigger is a strict
  subset of an existing one, and shipping it would report one span twice.
- **No `macro-generates-unbounded-defun-count` rule.** Asked for, and **dropped
  because the premise does not hold**: a macro expands at compile time, and its
  argument is whatever the call site literally wrote, so a macro looping over its
  own argument to emit definitions emits a count that *is* statically known at
  each call site. There is no "runtime-length sequence" at expansion time to
  detect.
- **No `generated-name-collides-with-export` rule.** Asked for, and **dropped
  because the shape it describes is the normal design**: generating the
  definitions of exported names from a macro is why the macro exists, so a
  generated name matching a `:export` entry is evidence the code is correct.
  Detecting it would also require correlating a `defpackage` with call sites
  elsewhere in the file — the exact per-invocation whole-file scan this package's
  cost rules forbid.
- **No `check-then-act` overlap.** That rule is
  `paredit-feature-lint-safety`'s, anchors on `unless`/`when`/`if`/`cond`, and
  is about a shared *place* written after being tested. It shares no head and no
  span with `introspection-probe-unchecked`.

## Dependencies

| Crate | Why |
| --- | --- |
| `paredit-core-lint-engine` | `LintRule`, `RuleMeta`, `HeadFilter`, `RuleContext`, `NormalizedHead`. |
| `paredit-core-syntax` | Rules match on parsed forms and on per-dialect operator spelling. |
| `paredit-core-cli` | Input reading, shared argument types, the report envelope. |
| `clap`, `serde_json` | Arguments and JSON output for each rule's own subcommand. |

## Layout

One rule, one directory — the four files a rule is made of, plus one shared
module:

```text
src/
├── support.rs           quote-aware traversal, the per-dialect operator tables
└── <rule>/
    ├── rule.rs          META, RULE, the head filter: what the registry registers
    ├── domain.rs        the detection itself
    ├── usecase.rs
    └── cli/             the `inspect <rule>` subcommand
```

`support.rs`'s `QuoteState`/`for_each_evaluated_subview`/`is_unevaluated_at`
group is copied from `paredit-feature-lint-testing`, tests included — a copy
rather than a dependency, because a feature→feature edge for a hundred lines of
traversal is not a trade worth making.

## When you change this package

| You are… | and it belongs here because… |
| --- | --- |
| adding a rule about a name or a definition built at run time | it is a new slice here, plus one line in the root's REGISTRY |
| teaching the suite another dialect's introspection spelling | `support.rs`'s `nil_returning_probes` / `apply_operators` |
| changing what one of the three flags, or how it phrases it | that rule's `domain.rs` |

| You are… | and it does **not** belong here because… |
| --- | --- |
| flagging an `eval`/`read-from-string` of computed data | that is `feature/lint-safety`'s `eval-of-non-constant` |
| flagging a test-then-write race on a shared place | that is `feature/lint-safety`'s `check-then-act` |
| flagging a macro that captures or leaks a binding | that is `feature/lisp-analysis`'s macro-hygiene report |
| asking which exported symbols are unused | that is `feature/package`'s export reports |
| changing how rules are dispatched or ordered | that is `core/lint-engine` |
