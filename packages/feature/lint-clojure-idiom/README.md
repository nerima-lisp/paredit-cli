# paredit-feature-lint-clojure-idiom

Four lint rules for Clojure: a resource scope whose value outlives the resource,
a namespace Var interned at call time, a nested-path accessor that reaches
through nothing, and an argument list written out only to be spread again.

The catalogue is overwhelmingly Common Lisp — before this package, Clojure had
two dedicated rules. Every rule here is `CLOJURE_ONLY`, and every rule declares
`HeadFilter::Heads`, so a file containing none of these heads never reaches a
single `check` body. That is what the `clean/forms/*` benchmarks measure, and it
is asserted by `a_file_with_none_of_these_heads_produces_no_findings`.

## The rules

| Rule | Category | Severity | Fixability | `Heads` |
| --- | --- | --- | --- | --- |
| `with-open-returns-lazy-seq` | `Resource` | `Error` | `ReportOnly` | `with-open` |
| `def-inside-function-body` | `Suspicious` | `Error` | `ReportOnly` | `defn`, `defn-`, `defmethod` |
| `single-key-nested-path` | `Allocation` | `Warning` | `ReportOnly` | `assoc-in`, `update-in`, `get-in` |
| `apply-with-literal-collection` | `Allocation` | `Warning` | `ReportOnly` | `apply` |

Nothing here ships a fix. Each repair is a choice the author has to make —
`doall` versus `into` versus restructuring; `let` versus a top-level
`defonce` — and `Fixability::ReportOnly` says so rather than picking one.

## Premises, and where they were checked

Each rule's claim was checked against a source outside this package, because a
rule's own tests only encode its author's model of the language.

- **`single-key-nested-path` is exactly clj-kondo's `:single-key-in`.** Same
  three heads, same shape test — a third child that is a vector of length one
  (`src/clj_kondo/impl/linters.clj`, `lint-single-key-in`). clj-kondo ships it
  at `:level :off` by default, which is the honest signal about how often it
  fires on deliberate code; see the audit below. The identity of the rewrite is
  derived from `clojure/core.clj` in the module's own documentation, arity by
  arity.
- **`def-inside-function-body` is clj-kondo's `:inline-def`**, which is on by
  default there. clj-kondo's own fixture `corpus/inline_def.clj` and the
  expectations in `test/clj_kondo/main_test.clj` served as an oracle: of the
  seven positions clj-kondo reports, this rule reports three and declines four
  by documented design (`(def foo (def x 1))`, `(t/deftest …)`, `defmacro`, and
  a bare top-level `fn`). Zero positions are reported that clj-kondo does not.
  Those four are false negatives, which is the direction this package errs in.
- **`with-open-returns-lazy-seq`'s vocabulary** was checked entry by entry
  against `clojure/core.clj`. `reductions` is lazy despite the name; `apply` is
  not a realizer, so `(apply concat (line-seq r))` stays reportable.
- **`apply-with-literal-collection`'s soundness boundary** — sets and maps are
  excluded because `apply` over `#{…}` spreads an unspecified order and over a
  map spreads `MapEntry` values, so the "write it directly" rewrite does not
  exist for either.

## Corpus audit

Author-written tests encode the author's model. The rules were therefore run
over real third-party Clojure with the built dispatcher, and every finding
adjudicated.

**Corpus**: 1714 `.clj`/`.cljc` files from `clojure/clojure`, `clj-kondo`,
`leiningen`, `reitit`, `malli`, `ring`, `core.async`, `next-jdbc`, `clj-http`,
`timbre`, `tools.cli`, `compojure`, plus the sources extracted from the jars in
`~/.m2`. 1690 parsed; 24 did not (babashka scripts, `#:ns{…}` namespaced map
literals, and two fixtures that are deliberately unreadable) — a
`paredit-core-syntax` limit, not this package's.

A zero-finding sweep over zero candidates proves nothing, so the denominator is
reported beside every rule: candidates are the occurrences that actually
head-matched and reached the rule's body.

| Rule | Candidates | Findings | Adjudication |
| --- | --- | --- | --- |
| `with-open-returns-lazy-seq` | 162 | 0 | 162 real resource scopes examined, none defective, no false positives |
| `def-inside-function-body` | 9910 | 5 | 5 true positives, 0 false positives |
| `single-key-nested-path` | 955 | 67 | true positives; equivalence holds, but see below |
| `apply-with-literal-collection` | 927 | 16 | true positives; 5 of 11 unique are Clojure's own tests *of* `apply` |

`with-open-returns-lazy-seq`'s zero is a real zero: 162 candidates were walked
and declined, most of them the correct `(->> (line-seq r) … (into []))`
spelling the rule exists to stay quiet on.

`def-inside-function-body`'s five are clj-kondo's own `:inline-def` fixture
(three), a deliberate inline `def` in Clojure's Var-metadata test suite, and
`(defn run-server [] (defonce server (ring/run-jetty …)))` in clj-http's test
helper — a genuine latent one.

`single-key-nested-path` fires on 7% of the nested-path calls in the corpus, 34
of them `update-in` with a one-key literal path. Every one is a true
equivalence, and the version-skew worry is unfounded — `update` arrived in
Clojure 1.7 and the projects that account for most of the findings target 1.9,
1.11 and 1.12. But the hit rate is why clj-kondo defaults its equivalent to
`:off`, and it is the reason this rule is `Warning` rather than `Error`.

### What the audit changed

Three defects, none of which any test in this package had caught:

1. **`case` test constants were read as code.** `case` never evaluates its test
   positions, and the reader marks them with nothing — so
   `(case tag (def defonce defmulti goog-define) …)` in clj-kondo's
   `analyzer.clj` is a list of four symbols that read as an inline `def`. Two
   false positives. Fixed in `support::ChildEvaluation`.
2. **`(comment …)` bodies were read as code.** `comment` expands to `nil`; its
   body never runs. Every applicable rule fired inside comment blocks — one
   corpus finding in clj-kondo's `cache.clj` scratch block, and clj-kondo's own
   test suite asserts `:inline-def` must be silent on exactly this shape. Fixed
   in the same place.
3. **`count-compared-to-zero` was dropped entirely.** See below.

Both fixes were checked for over-suppression against the corpus: they removed
exactly the three findings named above and changed no other rule's findings.

### The rule that was dropped

`count-compared-to-zero` reported `(zero? (count coll))`, `(= 0 (count coll))`,
`(pos? (count coll))` and the directional spellings, advising `empty?`/`seq`. It
passed its own 30-test suite and every fixture. The corpus refuted its premise.

- **The repair throws on `Counted`-but-not-`Seqable` receivers.** `count` accepts
  anything `Counted`; `seq` and `empty?` require seqability, and the two sets are
  not the same. `core.async`'s `FixedBuffer`, `DroppingBuffer` and `SlidingBuffer`
  are `deftype`s implementing `Counted` and nothing else, so `(seq buf)` throws
  `IllegalArgumentException`. Twelve findings — five in `channels.clj`, seven in
  `buffers_test.clj` — recommended code that does not run.
- **`clojure.core` disagrees with the rule.** `core.clj`'s definition of `empty?`
  is `(if (counted? coll) (zero? (count coll)) (not (seq coll)))` — the reported
  shape *is* `empty?`'s preferred implementation when the collection is counted.
  The rule flagged the body of the function it was recommending.
- **`clojure.core.reducers/cat` documents the shape as its contract** ("Tests for
  identity with `(zero? (count x))`").
- **Narrowing it to the sound subset leaves nothing.** The rule's strong claim —
  that `count` on a lazy sequence realizes everything — holds only when the
  receiver is known seqable. Restricted to a `count` whose argument is a
  syntactically known sequence-producing call, **0 of 55 findings survive**,
  across 11 091 candidate comparisons.

A rule that is wrong on 22% of its findings and empty on the rest is not a rule
to narrow. It was removed, along with `is_integer_zero_literal`, its only
consumer. Common Lisp's `length` has the analogous problem and its own package;
nothing here transfers to it.

## Cost

Measured with the shipped `atom-swap-with-side-effect` in the **same catalogue,
the same head index, the same parsed tree and the same dispatch pass** — it is
`CLOJURE_ONLY` too, so the comparison needs no second run. This box is shared
and sat at load average 41–50 throughout; absolute nanoseconds from it are not
comparable to anything, so only the ratios are claimed. Median of five runs,
2000 clean forms that head-match every rule and trigger none.

| Rule | ns/invocation | × control |
| --- | --- | --- |
| `atom-swap-with-side-effect` (control) | 28.8 | 1.00× |
| `apply-with-literal-collection` | 20.3 | 0.71× |
| `single-key-nested-path` | 39.2 | 1.39× |
| `def-inside-function-body` | 184.0 | 6.6× |
| `with-open-returns-lazy-seq` | 337.1 | 11.3× |
| package aggregate | 126.0 | 4.4× |

`def-inside-function-body` is the only rule that examines a whole subtree per
head match. It previously cost ~30× the control; `may_be_definition_head`, a
three-byte pre-filter ahead of four `symbol_in` calls per node, is what brought
it to 6.6×. `with-open-returns-lazy-seq` is the most expensive per invocation
and the cheapest in practice: `with-open` occurred 162 times in 1690 real files.

Doubling the file doubles the work — total ×1.85–2.48, per-invocation
×0.80–1.24 across all five runs and every rule. Nothing here is superlinear,
which is the property `doubling_the_file_does_not_more_than_triple_the_cost`
guards in CI.

## Mutation testing

Every guard was removed in turn, rebuilt, and the suite re-run: 41 mutations, 36
killed. The five survivors are cost guards, not behaviour guards, and each is
documented as such in `support.rs`. Three gaps the exercise found and closed:

- A rule's `HeadFilter` was not pinned to its domain's head list. Deleting
  `defmethod` from `def-inside-function-body` left the whole suite green,
  because the domain tests call `build_…_report` — which walks the tree itself
  and never consults the dispatcher's head index. The rule would have stopped
  seeing every `defmethod` in production with nothing failing.
- `is_realization_barrier`'s bare-threading-stage branch, the arity floor on
  `apply`, the metadata skip when naming an applied function, and the
  binding-vector check on `with-open` were all unkilled; each now has a test.
- Two guards were dead code and are gone: a `HashLiteral` exclusion in
  `is_vector_literal` that no Clojure syntax can reach, and a
  `!is_vector_literal(last)` check in `tail_forms` that `list_head`'s
  paren-only gate already made unreachable.

`may_be_definition_head` also survives, and must: it is an *exact* pre-filter,
so a mutation that disables it is required to change nothing. Its exactness has
its own assertion instead.

## Registration

This package is deliberately **unregistered**. Its rules are named by the root
crate's `REGISTRY`; until they are, the only thing exercising them is this
package's own `engine_pass_tests`, which builds a `RuleCatalog` and runs the
real dispatcher over it.
