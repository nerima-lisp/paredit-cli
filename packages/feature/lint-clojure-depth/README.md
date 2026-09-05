# paredit-feature-lint-clojure-depth

Four lint rules for Clojure, one layer below `lint-clojure-idiom`: `core.async`
go-block discipline on both sides of a thread boundary, a `contains?` whose
answer is fixed before it runs, and the reference types' operators crossed
over.

Every rule here is `CLOJURE_ONLY` and every rule declares `HeadFilter::Heads`,
so a file containing none of these heads never reaches a single `check` body.
That is what the `clean/forms/*` benchmarks measure, and it is asserted by
`a_file_with_none_of_these_heads_produces_no_findings`.

The dialect scope matters more here than in the sibling package: `let`, `loop`
and `go` are all Common Lisp operators — `go` is `tagbody`'s transfer, which
takes a *tag* rather than a body — so a rule that lost its override would not
merely run, it would run over a great deal of Common Lisp with a Clojure
vocabulary. `the_rules_are_silent_on_the_same_bytes_read_as_common_lisp` is the
test that says so.

## The rules

| Rule | Category | Severity | Fixability | `Heads` |
| --- | --- | --- | --- | --- |
| `go-block-blocking-channel-op` | `Concurrency` | `Error` | `ReportOnly` | `go`, `go-loop` |
| `parking-op-outside-go-machinery` | `Concurrency` | `Error` | `ReportOnly` | `go`, `go-loop` |
| `contains-on-non-associative` | `Suspicious` | `Error` | `ReportOnly` | `contains?` |
| `reference-type-operator-mismatch` | `Concurrency` | `Error` | `ReportOnly` | `if-let`, `if-some`, `let`, `let*`, `loop`, `when-let`, `when-some` |

Nothing here ships a fix. `>!!` → `>!` is the repair *most* of the time and
`(thread …)` the rest of it; `contains?` over a sequence becomes `some`, a set,
or a different data structure; a crossed reference operator is repaired either
by changing the operator or by changing the constructor, and which one is right
depends on what coordination the code needs. `Fixability::ReportOnly` says so
rather than picking one.

The two `go` rules are opposite halves of one walk.
`go-block-blocking-channel-op` reports the `!!` operations *in front of* a
thread boundary; `parking-op-outside-go-machinery` reports the parking
operations *behind* one. Neither can produce the other's finding, and
`support::for_each_call_across_boundaries` carries the boundary rather than
pruning on it precisely because a pruning walk can only express one of the two.

## Premises, and where they were checked

Each rule's claim was checked against a primary source outside this package,
because a rule's own tests only encode its author's model of the language. No
`clj`/`clojure` binary exists on this machine (`which clj`, `which clojure`,
`ls /nix/store/*clojure*` — only tree-sitter grammars and Emacs modes), so
every premise below is read off the shipped source of `clojure/clojure` and
`clojure/core.async` rather than executed.

### `go-block-blocking-channel-op`

Two questions had to be settled before the rule was worth writing, and the
second is the one that decides it.

**Is blocking in a `go` block actually a defect?** `go`'s own docstring says so
(`async.clj`:493-497):

> go blocks should not (either directly or indirectly) perform operations that
> may block indefinitely. Doing so risks depleting the fixed pool of go block
> threads, causing all go block processing to stop. This includes core.async
> blocking ops (those ending in !!) and other blocking IO.

**Does core.async or the compiler already catch it — the way the Common Lisp
`loop` `collect x sum x` shape turned out to be a macroexpansion error?** It
ships a detector, and the detector is *opt-in, runtime, and off by default*.
`doc/reference.md`:120-124:

> core.async includes a debugging facility to detect this situation (other
> kinds of blocking operation cannot be detected so this covers only part of
> the problem). To enable go checking, set the Java system property
> `clojure.core.async.go-checking=true`. This property is read once, at
> namespace load time, and should be used in development or testing, not in
> production.

The whole of it is `defblockingop` (`async.clj`:150-159), which wraps `<!!`,
`>!!` and `alts!!` in a `dispatch/check-blocking-in-dispatch` **only** when
`(Boolean/getBoolean "clojure.core.async.go-checking")` is true at the time
`clojure.core.async` itself loads; `check-blocking-in-dispatch`
(`impl/dispatch.clj`:57-61) throws `IllegalStateException` off a `ThreadLocal`
marker that `with-dispatch-thread-marking` (`dispatch.clj`:42-50) only sets
under the same property.

Statically, nothing. `go` expands through
`clojure.core.async.impl.go/go-impl` (`async.clj`:505-506, `go.clj`:1044-1059),
which builds the state machine and never inspects the body for blocking
operations. So the premise **survives**: a static rule is complementary to a
facility that is off in production and that its own documentation says "covers
only part of the problem".

### `parking-op-outside-go-machinery`

`<!`, `>!` and `alts!` are not functions that do anything. Each is a `defn`
whose entire body is an assertion that the `go` transform rewrote it away
(`async.clj`:174-178, 213-218, 358-382):

```clojure
(defn <! [port] (assert nil "<! used not in (go ...) block"))
(defn >! [port val] (assert nil ">! used not in (go ...) block"))
(defn alts! [ports & {:as opts}] (assert nil "alts! used not in (go ...) block"))
```

So a parking operation that survives the transform is an `AssertionError` on
first execution — or, with `*assert*` false at compile time, silently `nil`,
which is worse. `alt!` (`async.clj`:429-432) is the macro over `alts!` and
carries the same "Must be called inside a (go ...) block".

The boundary list (`support::THREAD_BOUNDARY_HEADS`) is the rest of the rule,
and every entry is a form whose macroexpansion puts an `fn*` around its body,
which the state machine — built from *the body it is handed* — cannot reach
into. The asymmetry that matters:

| form | expansion | in a `go` body |
| --- | --- | --- |
| `doseq` | nested `loop`/`recur`, no `fn*` (`core.clj`:3240-3290) | `(go (doseq [x xs] (>! c x)))` **works** |
| `for` | a lazy sequence through `fn*` | `(go (for [x xs] (<! x)))` **breaks** |
| `thread` | `` `(thread-call (^:once fn* [] ~@body) :mixed) `` (`async.clj`:531-536) | breaks for parking, **required** for blocking |
| `dosync` | `` `(. LockingTransaction (runInTransaction (fn [] ~@body))) `` | breaks |

`letfn` is deliberately **absent**: its *body* is in the enclosing scope, so
calling the whole form a boundary would report a parking op that is fine.

### `contains-on-non-associative`

`contains?` is `(. clojure.lang.RT (contains coll key))` (`core.clj`:1502-1510)
and `RT.contains` (`RT.java`:824-848) is a closed dispatch over `Associative`,
`IPersistentSet`, `java.util.Map`, `java.util.Set`, an indexed `String`/array
with a numeric key, and the two transient interfaces — falling through to

```java
throw new IllegalArgumentException("contains? not supported on type: " + coll.getClass().getName());
```

`PersistentList`, `LazySeq`, `Cons`, `Range` and `LongRange` all reach
`ASeq`/`Obj` implementing `ISeq, Sequential, List` — `java.util.List`, which is
neither `Map` nor `Set` — so none of the branches matches and the call throws.
A producer that answered `nil` instead (`(keys {})`, `(seq [])`) takes the
first branch and returns `false`. Hence the uniform claim: **`contains?` over a
sequence can never answer true.** `contains?`'s own docstring says why one
reaches for it anyway — "it will not perform a linear search for a value. See
also 'some'."

The second shape is `APersistentVector.containsKey`
(`APersistentVector.java`:387-392):

```java
if(!(Util.isInteger(key))) return false;
```

so `(contains? [:a :b] :a)` is `false`, silently, with no exception to notice.
A vector's keys are its indexes.

`shuffle` and `split-at` are absent from `support::SEQ_PRODUCER_HEADS` on
purpose: `shuffle` returns `(RT/vector (.toArray al))` and `split-at` a
two-element vector, so neither reaches that `throw`.

### `reference-type-operator-mismatch`

The four reference containers and the operators that type-check against them:

| constructor | operators | first parameter |
| --- | --- | --- |
| `(atom …)` | `swap!`, `swap-vals!`, `reset!`, `reset-vals!`, `compare-and-set!` | `clojure.lang.IAtom` |
| `(ref …)` | `alter`, `commute`, `ref-set`, `ensure` | `clojure.lang.Ref` |
| `(agent …)` | `send`, `send-off`, `send-via` | `clojure.lang.Agent` |
| `(volatile! …)` | `vswap!`, `vreset!` | `clojure.lang.Volatile` |

Crossing them is a `ClassCastException` at the first call. `volatile!` is where
this is least obvious: it looks like an atom, reads like an atom, is what a
stateful transducer holds — and `swap!` on one throws.

`send`, `send-off` and `send-via` are deliberately **not** reported. They are
the agent operators, and `send` is also an ordinary name for a user function
over a connection or a socket, which this rule cannot distinguish —
`(let [conn (atom nil)] (send conn "hi"))` is correct code under any number of
libraries. An agent is therefore only ever detected as the *target* of an atom,
ref or volatile operator.

## Rejected rule candidates

The following candidates are not implemented for the reasons given below.

### `defrecord-implements-protocol-method-not-in-protocol` — refuted by the compiler

`Compiler.java`:9086 computes
`findMethodsWithNameAndArity(name.name, parms.count(), overrideables)` for every
method of a `deftype`/`defrecord`/`reify`, and line 9128 is

```java
throw new IllegalArgumentException("Can't define method not in interfaces: " + name.name);
```

A method whose *name and arity* match no interface method the type implements
is a **compile-time error**. The only surviving variant — a method that belongs
to protocol B but is written under protocol A's heading — is a matter of layout
and not a defect. This is the `collect x sum x` case exactly: the compiler
already rejects the shape, so the rule would be worthless.

### `transducer-used-as-sequence-function` — not decidable

`(map inc)` and `(map inc coll)` differ by arity, and passing the arity-1 form
to `into`/`sequence`/`transduce`/`eduction` is *correct*. The genuinely broken
shapes — `(into [] (map inc))` with the collection forgotten, `(into [] (map
inc xs) ys)` with an already-applied sequence call where a transducer belongs —
are provable but are typos with no measured occurrence, and everything else
needs to know what the function argument *is*, which is a type this dialect
does not carry. There is no decidable shape to audit against a corpus.

### `reduce-without-init-on-possibly-empty` — refuted by measurement

`(reduce f coll)` calls `(f)` only when `coll` is empty, and whether it can be
is not visible at the call site. The corpus settles the noise question:

| `(reduce …)` call sites | count |
| --- | --- |
| 3 arguments (has an init) | 872 |
| **2 arguments (no init)** | **172** |
| of those, `f` is a literal `(fn …)` / `#(…)` — the narrowest provable form | **35** |

172 no-init reduces in shipped, working Clojure, and 35 with an inline function
that demonstrably has no zero-arity: `(reduce #(and %1 %2) all-true)` in
Clojure's own `test_clojure/sequences.clj`:84, `(reduce (fn [m ^String line] …)
lines)` in `cognitect/aws/config.clj`:67, `(reduce #(-unify* init-s %1 %2)
(butlast ts))` in `core.logic/unifier.clj`:123. Every one is correct because
the collection is never empty, and nothing in the call says so. Even the
narrowest version of this rule is 35 false positives — the same shape as the
"104 occurrences of the idiom in correct Common Lisp" finding that killed an
earlier rule.

### `spec-fdef-arity-mismatch` and `protocol-method-missing-arity` — structurally too expensive

Both are cross-form correlations: `s/fdef` ↔ `defn`, `defprotocol` ↔
`defrecord`. Neither is refuted — a `defprotocol` arity the `defrecord` does
not implement is a genuine `AbstractMethodError` at run time, and the compiler
does **not** catch it (`findMethodsWithNameAndArity` rejects methods that are
*not* in an interface; it says nothing about ones that are missing).

There is no non-quadratic way to build them here.
Correlating two top-level forms needs a per-file index, and
`RuleContext::scratch_cache` is a single type-erased slot already owned by
`lint-repl-debug` — a second type in it panics. Without it, each `defrecord`
must re-scan the top level for its `defprotocol`, which is O(n) per candidate
and O(n²) per file *by construction*, before any measurement. The precedent is
exact: a `defgeneric`↔`defmethod` congruence rule in this repository passed its
corpus audit clean over 765 pairs and was still dropped at 4.37 s for 2000
protocols — slower than the scan it was written to beat. The `s/fdef` case is
worse besides: `s/fdef` need not name a function in the same file at all.

### `dynamic-scope-returns-lazy-seq` — rejected after implementation

Heads `binding`, `with-bindings`, `with-local-vars`, `with-redefs`. Its premise
is *correct* and was verified: all four restore what they installed in a
`finally`, so the body's value reaches the caller after the scope is gone —
`binding` at `core.clj`:1980-1985, `with-local-vars` at 4375-4380,
`with-redefs`/`with-redefs-fn` at 7791-7809 and 7811-7824. None of the four
docstrings mentions laziness; the macroexpansion is the whole of the evidence.

It was implemented, tested, and run over the corpus, and the corpus killed it:

| candidates | findings | true positives | false positives |
| --- | --- | --- | --- |
| 978 | 1 | 0 | 1 |

The one finding is `pedestal/build/build.clj`:56 — `(defn- classpath-for [dir
overrides] (binding [b/*project-root* dir] … (map (fn [path] …) roots)))`. The
mapping function closes over `dir` and calls `str/starts-with?` and `str`, and
`roots` is an already-realized vector, so the value provably does not observe
the dynamic scope. A false positive.

The defect the rule exists for requires the lazy computation to *read* the
dynamic Var, and that is exactly what cannot be seen: `(binding [*ns* n] (map
eval forms))` is broken and `(binding [*print-length* 5] (map inc xs))` is
fine, and they are the same shape. Narrowing to `with-redefs`/`with-bindings`
alone — whose entire purpose is to change what the body computes — was
considered and rejected: it has the same undecidable core with better priors,
and it produced zero findings, so there is no evidence it earns a catalogue
entry either. A rule whose only real-world finding is wrong does not ship.

## Duplicate sweep

Every rule in the catalogue whose scope includes Clojure, read by its
`examine_*` body rather than by its name:

| Rule | Package | `Heads` | What its body detects | Overlap |
| --- | --- | --- | --- | --- |
| `with-open-returns-lazy-seq` | `lint-clojure-idiom` | `with-open` | a `with-open` whose tail is an unrealized `clojure.core` sequence call reaching the bound resource | none |
| `def-inside-function-body` | `lint-clojure-idiom` | `defn`, `defn-`, `defmethod` | a `def`/`defonce` interned from inside a function body | none |
| `single-key-nested-path` | `lint-clojure-idiom` | `assoc-in`, `update-in`, `get-in` | a literal one-element path vector (`single_key_nested_path/domain.rs`:190) | none |
| `apply-with-literal-collection` | `lint-clojure-idiom` | `apply` | `apply` over an ordered literal sequence | none |
| `nested-get-chain` | `lint-sequence` | `get` | `(get (get m :a) :b)`, which is `get-in` (`nested_get_chain/domain.rs`:1-8) | none |
| `redundant-into-empty-collection` | `lint-sequence` | `into` | an `into` whose target already is that collection | none |
| `atom-swap-with-side-effect` | `lint-concurrency` | `swap!`, `swap-vals!`, `alter`, `commute` | an **inline** `(fn …)`/`#(…)` update function containing a side effect, which `swap!`'s retry loop repeats (`atom_swap_with_side_effect/domain.rs`:1-24) | **none — see below** |
| `future-promise-never-realized` | `lint-concurrency` | `let`, `loop`, `if-let`, `when-let`, `binding` | a binding to `(future …)`/`(future-call …)`/`(promise)`/`(delay …)` whose symbol never occurs in the body (`future_promise_never_realized/domain.rs`:11-16) | **none — see below** |

Three more rules include Clojure in a wider scope and share no head with this
package: `todo-fixme-no-attribution` (`lint-documentation`),
`leftover-print-debug` (`lint-repl-debug`) and `self-recursive-tail-call`
(`lint-control-flow`).

Two are close enough to need the bodies read, and both turn out not to be
duplicates — in both directions:

- **`atom-swap-with-side-effect` shares four operator names with
  `reference-type-operator-mismatch`** (`swap!`, `swap-vals!`, `alter`,
  `commute`) and shares no finding with it. Its *heads* are those operators;
  ours are the binding forms. It asks whether the update function is pure; we
  ask whether the first argument is the right kind of reference. On
  `(let [r (ref 0)] (swap! r inc))` it is silent, because `inc` is a name it
  cannot follow (`atom_swap_with_side_effect/domain.rs`:14-17), and we report.
  On `(swap! a (fn [x] (println x) x))` it reports and we are silent. Deleting
  either deletes findings the other never makes.

- **`future-promise-never-realized` shares four of our seven heads** (`let`,
  `loop`, `if-let`, `when-let` — `future_promise_never_realized/rule.rs`:36-40)
  and the whole anchoring pattern: head on the binding form, pre-filter on a
  constructor call in the binding vector, then walk the body. The constructor
  sets are disjoint — `future`/`future-call`/`promise`/`delay` against
  `atom`/`ref`/`agent`/`volatile!` — and the verdicts are unrelated: theirs is
  "the symbol never occurs in the body", ours is "an operator of the wrong kind
  is applied to it". It is also the precedent that makes `let` an affordable
  head at all: a shipped rule already anchors there, with the same pre-filter
  shape, for the same reason.

`set-membership-via-linear-scan` (`lint-sequence`) is the nearest conceptual
neighbour of `contains-on-non-associative` and is **`COMMON_LISP_ONLY`** with
head `member` (`set_membership_via_linear_scan/rule.rs`:47, 97-98). It reports a
membership test that is *slow*; ours reports one that is *impossible*. No
overlap, and no head in common.

## Corpus audit

Author-written tests encode the author's model. The rules were therefore run
over real third-party Clojure through the **built dispatcher** —
`build_head_index` plus `collect_lint_outcomes`, so a wrong `HeadFilter` or a
forgotten dialect scope would show up as a silent zero — and every finding
adjudicated.

**Corpus**: 3035 `.clj`/`.cljc` files (`.cljs` excluded) from 46 repositories —
`clojure/clojure`, `core.async`, `spec.alpha`, `tools.deps`, `tools.build`,
`tools.namespace`, `tools.cli`, `tools.logging`, `data.json`, `java.jdbc`,
`test.check`, `core.match`, `core.logic`, `algo.monads`, `clj-kondo`, `reitit`,
`malli`, `ring`, `next-jdbc`, `integrant`, `mount`, `promesa`, `pedestal`,
`onyx`, `aleph`, `chime`, `nrepl`, `martian`, `leiningen`, `clj-http`, `timbre`,
`compojure`, `hiccup`, `babashka`, `sci`, `orchard`, `duct`, `clip`, `jsonista`,
`muuntaja`, `http-kit`, `specter`, `aws-api`, `coax`, `schema`, `nippy` — plus
the sources extracted from the jars in `~/.m2`.

**2952 parsed. 83 did not (2.7%).** By cause:

| cause | files |
| --- | --- |
| `unsupported reader dispatch '#'` at byte 0 — a `#!/usr/bin/env bb` shebang | 44 |
| `unsupported reader dispatch '#'` elsewhere — `#=`, `#^` old-style metadata, tagged literals | 23 |
| `unsupported reader dispatch '#?'` — a reader conditional in a `.clj` (not `.cljc`) file | 5 |
| unbalanced or mismatched delimiters — deliberately-unreadable test fixtures | 10 |
| unterminated string — a deliberately-unreadable test fixture | 1 |

The shebang class is the large one and is a `paredit-core-syntax` limit, not
this package's; `#^` remains unsupported as previously recorded.

A zero-finding sweep over zero candidates proves nothing, so the denominator is
reported beside every rule. Candidates are the occurrences that actually
head-matched and reached the rule's body.

| Rule | Candidates | Findings | True positives | False positives |
| --- | --- | --- | --- | --- |
| `go-block-blocking-channel-op` | 112 go blocks | 1 | **1** | 0 |
| `parking-op-outside-go-machinery` | 112 go blocks | 0 | — | 0 |
| `contains-on-non-associative` | 900 `contains?` calls | 0 | — | 0 |
| `reference-type-operator-mismatch` | 606 reference bindings | 0 | — | 0 |

### The one true positive

`onyx/src/onyx/api.clj`:723, in `await-job-completion`:

```clojure
(let [[v c] (alts!! [(go (let [entry (<!! ch)]
                           (extensions/apply-log-entry entry …)))
                     tmt]
                    :priority true)]
```

A `go` block whose first act is a **blocking** take from `ch`, inside a `loop`
that re-enters it once per log entry. The go-dispatch thread is held for as
long as the channel is empty, which is the exact shape `go`'s docstring warns
depletes the pool. `<!` is the operator that belongs there. Reported once, with
no false positive beside it.

### The three zeros

None is a false clean: 900, 606 and 112 real occurrences were walked and
declined, most of them the correct spellings these rules exist to stay quiet on
— `(contains? #{…} x)` and `(contains? m :k)`, `(swap! an-atom …)` beside
`(dosync (alter a-ref …))`, and parking ops in a `go` body beside `<!!` inside
a `thread`.

All three detect a *guaranteed host exception* — `IllegalArgumentException`,
`ClassCastException`, `AssertionError` — and mature, widely-deployed libraries
have long since had those shaken out. Firing on a shipped corpus is not the
bar; firing on the `DANGEROUS_TWIN` through the real dispatcher, exactly once
each, is (`the_dangerous_twin_fires_every_rule_exactly_once`).

### The head-normalization gap, measured

`engine::head_index::head_key` returns a Clojure head **verbatim** — the
package-qualifier folding is `dialect == Dialect::CommonLisp` only — and
`view_query::unqualified` splits on `:`, not on Clojure's `/`. So `a/go` does
not normalize to `go` and the two `go` rules never see it.

Measured over the corpus, by spelling:

| spelling | occurrences |
| --- | --- |
| bare `go` / `go-loop` | 140 (82%) |
| `a/go`, `async/go`, `sp/go`, `cljs.core.async.macros/go`, `clojure.core.async/go` | 30 (18%) |

So roughly one `go` block in six is invisible to this package. That is a
**core-engine gap**, reported rather than fixed: the correct place for it is
`head_key`, in `packages/core/lint-engine`, outside this crate.
Adding `a/go` and `async/go` to the `Heads` list was considered and rejected —
an alias is arbitrary, and a head list keyed to a naming convention can only be
tested against that convention. A missed alias-qualified `go` is a false
negative, which is the direction this package errs in throughout.

Clojure's value and type semantic layers are unavailable here for the same
reason: `semantics::value::policy::dialect_gate::supports_value_propagation` and
`semantics::typing::policy::dialect_gate::supports_type_inference` are Common
Lisp / Emacs Lisp only (plus Scheme and Racket for typing) and return `false`
for Clojure. A reference-kind analysis richer than the lexical one in
`reference-type-operator-mismatch` would need that layer.

## Cost

`PassOptions { measure: true }`, two file sizes, this package's four rules
measured in the **same process and over the same generated file** as four
shipped rules from `lint-clojure-idiom`. Best-of-two per size, taken
interleaved. The machine was loaded — `uptime` reported a one-minute load
average of 23–36 across the runs — which is why the absolute nanoseconds are
not the point and the ratio is.

**Worst case: every head has a candidate.**

| rule | ns/call @500 | ns/call @1000 | ratio |
| --- | --- | --- | --- |
| `contains-on-non-associative` | 39.0 | 34.1 | 1.75 |
| `go-block-blocking-channel-op` | 659.0 | 572.9 | 1.74 |
| `parking-op-outside-go-machinery` | 632.3 | 548.6 | 1.74 |
| `reference-type-operator-mismatch` | 2612.7 | 2272.1 | 1.74 |
| *(shipped)* `single-key-nested-path` | 64.5 | 56.4 | 1.75 |
| *(shipped)* `def-inside-function-body` | 410.9 | 412.7 | 2.01 |
| *(shipped)* `apply-with-literal-collection` | 36.8 | 31.0 | 1.68 |
| *(shipped)* `with-open-returns-lazy-seq` | 507.0 | 439.3 | 1.73 |

**Common case: every head present, no candidate** — the shape a real file has,
and what the `clean/forms/*` benchmarks are about.

| rule | ns/call @500 | ns/call @1000 | ratio |
| --- | --- | --- | --- |
| `contains-on-non-associative` | 35.7 | 35.8 | 2.01 |
| `go-block-blocking-channel-op` | 647.5 | 621.7 | 1.92 |
| `parking-op-outside-go-machinery` | 623.0 | 599.7 | 1.93 |
| `reference-type-operator-mismatch` | **76.9** | **74.4** | 1.94 |
| *(shipped)* `single-key-nested-path` | 61.9 | 60.5 | 1.95 |
| *(shipped)* `def-inside-function-body` | 328.2 | 358.8 | 2.19 |
| *(shipped)* `apply-with-literal-collection` | 31.8 | 31.7 | 2.00 |
| *(shipped)* `with-open-returns-lazy-seq` | 493.9 | 455.1 | 1.84 |

**No rule here is quadratic.** Every ratio is 1.74–2.01, inside the 1.68–2.19
band the four shipped rules produce in the same runs. A per-candidate
`tree.root_view()`, or a top-level rescan of the kind the two dropped
cross-form rules would have needed, reads as ~4.0 and does not appear.

`reference-type-operator-mismatch` has the largest candidate-dependent cost:
**2613 ns/call when the binding vector holds a reference constructor, 77 when
it does not.** `let` is the most common head in the language, and the rule is
affordable only because the body is never walked until a constructor has been
found — a delimiter test plus one `list_head` per init, allocating nothing.
That ordering is the rule's entire cost story, and `REF5` in the mutation table
below is the guard that expresses it.

The same ordering governs `support::is_unevaluated_at`, which materializes the
enclosing top-level form: every `check` here calls it **only after it already
holds a candidate finding**, never as a precondition on the head match. A
sibling package measured 450843 ns/call against 28 ns/call purely from getting
that backwards. `root_child_containing` binary-searches with
`SyntaxTree::root_child_span` rather than `Path::root_child`, which
heap-allocates once per probe.

## Mutation results

Every guard was removed, rebuilt, run, and restored — the file rewritten and
`touch`ed rather than moved, because an older mtime lets cargo reuse a stale
binary. The kill signal is the *exit code* of `cargo test` plus the named
failing tests, not a grep of its output: `error: test failed` is what a failing
test prints, so grepping stderr for "error" reports a red suite as a compile
error. 24 mutants: **19 killed, 4 survive correctly, 1 was dead code and is
gone.**

| mutant | result |
| --- | --- |
| a reader lambda must be a paren list (`#{…}`/`#?(…)` are not) | killed by 1 |
| the boundary applies at the reader-lambda node itself, not only to its children | killed by 6 |
| `(comment …)` bodies are data | killed by 8 |
| `case` test constants are data | killed by 2 |
| the top-level containment check in `root_child_containing` | killed by 2 |
| the per-child containment check in `child_containing` | killed by 1 |
| `'` latches `hard` rather than counting `quasi` | **killed by 2 — see below** |
| prune at a nested `go` (blocking rule) | killed by 2 |
| skip anything behind a thread boundary (blocking rule) | killed by 3 |
| prune at a nested `go` (parking rule) | killed by 1 |
| report only behind a boundary (parking rule) | killed by 8 |
| the `contains?` arity check | killed by 1 |
| the key must be a keyword or string literal | killed by 2 |
| the collection must be a literal vector | killed by 4 |
| the shadowing prune | killed by 2 |
| only even indices of a binding vector introduce names | killed by 1 |
| the last binding of a name wins | killed by 1 |
| the binding vector must be a vector literal | **killed by 2 — see below** |
| an operator's target is its first argument | killed by 13 |
| childless children are not queued (boundary walk) | **survives: cost only** |
| the `go` node itself is not walked as a body child | **survives: cost only** |
| the empty-`tracked` early return | **survives: cost only** |
| childless children are not queued (reference body walk) | **survives: cost only** |
| the key must be an atom | **survived — dead code, removed** |

Three needed chasing rather than recording:

1. **`'` latches `hard` rather than counting `quasi`** initially survived. The
   two-counter model's whole point is that `~` inside a *hard* quote is not an
   escape — the reader produces a literal `(clojure.core/unquote x)`, still
   data — and no test in this package covered `'(a ~(…))`, only
   `` `(a ~(…)) `` and the Clojure comma-is-whitespace case. That was a real
   gap in the copied machinery;
   `a_tilde_inside_a_hard_quote_does_not_escape_back_to_code` closes it.

2. **The binding vector must be a `[…]` vector literal** initially survived,
   because no test fed a non-vector binding position. Without the filter,
   `(let (a (atom 0)) (alter a inc))` — Common Lisp pasted into a `.clj` file —
   has its paren list read as `name init` pairs and invents a binding.
   `a_binding_position_that_is_not_a_vector_binds_nothing` closes it.

3. **`is_non_index_literal`'s `if !view.children.is_empty() { return None; }`**
   survived and could not have done otherwise: `atom_text` answers `Some` only
   for an `ExpressionKind::Atom`, and every node with children is a list. No
   test can distinguish it. It was dead and is deleted.

A fourth guard was removed *before* the sweep for the same reason:
`names_bound_by` opened with `if is_reader_lambda(view) { return Vec::new(); }`,
on the theory that `#(…)`'s parameters are `%` and cannot shadow. A `#(…)` has
no parameter vector at all — its own `list_head` is whatever its *body* starts
with — so the branch was unreachable except when that body is itself a binding
form, where it was actively **wrong**: `#(let [a (ref 0)] (alter a %))` does
shadow `a`. Removed, and
`a_reader_lambda_shadows_nothing_but_a_binding_form_inside_one_does` now pins
both halves.

The four surviving guards survive correctly: deleting each changes cost and not
one finding. `REF5` — the `if tracked.is_empty() { return; }` in
`examine_reference_bindings` — is the largest of them by far, and is the 77 ns
against 2613 ns in the cost table above.

## Where this package errs

Toward false negatives, everywhere:

- an alias-qualified `go` (18% of them, measured above);
- a parking op with no enclosing `go` at all — `(defn take-one [c] (<! c))` is
  the same runtime assertion, and finding it means asking a `<!` node about its
  ancestors, which is the per-candidate top-level materialization this package
  refuses;
- blocking IO in a `go` block that is not a channel op — `(Thread/sleep 1000)`,
  a JDBC call, `@(future …)` — which core.async's own documentation concedes it
  cannot detect either;
- a parking op inside a `letfn` function definition;
- a reference cell held anywhere but a lexical binding: a top-level
  `(def counter (atom 0))` is cross-form and therefore quadratic;
- an agent detected by an agent operator, since `send` is not a name this rule
  can claim;
- a `contains?` over a user function's result, or over a plain symbol;
- any name rebound anywhere under a tracked binding, because the shadowing
  guard prunes the whole subtree rather than tracking the name through it.
