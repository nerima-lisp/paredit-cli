#![doc = include_str!("../README.md")]

pub mod contains_on_non_associative;
pub mod go_block_blocking_channel_op;
pub mod parking_op_outside_go_machinery;
pub mod reference_type_operator_mismatch;
pub mod support;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// Two declarations decide whether a rule is reachable from the CLI at all,
/// and neither is visible to a domain test, which calls `examine_*` on a node
/// it picked itself:
///
/// - the `HeadFilter::Heads` list, which is what the dispatcher's head index
///   is built from. A head spelled wrongly — or a head the domain matches but
///   the list omits — leaves every `examine_*` test green while the rule never
///   receives a single node in production.
/// - the [`RuleDialectScope`], which the dispatcher consults *before* walking
///   anything. Every rule here is `CLOJURE_ONLY`. That matters more in this
///   package than in its sibling: `let`, `loop`, `binding` and `go` are all
///   Common Lisp operators too, so a rule that lost its override would not
///   merely run — it would run over a *lot* of Common Lisp with a Clojure
///   vocabulary.
///
/// [`RuleDialectScope`]: paredit_core_lint_engine::policy::RuleDialectScope
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{
        PassOptions, build_head_index, collect_lint_outcomes, collect_lint_pass,
    };
    use paredit_core_lint_engine::model::HeadFilter;
    use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::contains_on_non_associative::rule::META,
            &crate::contains_on_non_associative::rule::RULE,
        ),
        RuleEntry::new(
            &crate::go_block_blocking_channel_op::rule::META,
            &crate::go_block_blocking_channel_op::rule::RULE,
        ),
        RuleEntry::new(
            &crate::parking_op_outside_go_machinery::rule::META,
            &crate::parking_op_outside_go_machinery::rule::RULE,
        ),
        RuleEntry::new(
            &crate::reference_type_operator_mismatch::rule::META,
            &crate::reference_type_operator_mismatch::rule::RULE,
        ),
    ];

    /// A Clojure source that triggers exactly one of these rules, per rule.
    ///
    /// Each is written to miss the other three, which is not automatic: the
    /// two `go` rules share both their heads *and* their walk, so a `go`
    /// trigger for one must contain no operator the other reports.
    const TRIGGERS: [(&str, &str); 4] = [
        ("contains-on-non-associative", "(contains? (keys m) :id)"),
        ("go-block-blocking-channel-op", "(go (>!! out v))"),
        (
            "parking-op-outside-go-machinery",
            "(go (map (fn [c] (<! c)) chs))",
        ),
        (
            "reference-type-operator-mismatch",
            "(let [r (ref 0)] (swap! r inc))",
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.clj"),
            dialect,
            &tree,
            source,
            RuleSelection::All,
        )
        .expect("lint pass")
        .into_iter()
        .map(|outcome| outcome.into_parts().0.rule)
        .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    // -- (a) each rule is reached through the head index -----------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        for (rule, source) in TRIGGERS {
            assert_eq!(
                fired(source, Dialect::Clojure),
                vec![rule],
                "{rule} is unreachable through the head index"
            );
        }
    }

    /// Four rules, four distinct names: a copy-paste in `ENTRIES` that
    /// registered one slice twice would otherwise leave the loop above green.
    #[test]
    fn the_catalog_holds_all_four_rules_once_each() {
        let mut names: Vec<&'static str> = RuleCatalog::new(&ENTRIES)
            .entries()
            .iter()
            .map(|entry| entry.meta().name().as_str())
            .collect();
        assert_eq!(names.len(), 4);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 4);
    }

    // -- (b) the corpus sweep --------------------------------------------------

    /// A realistic, *correct* Clojure namespace that exercises every head this
    /// package anchors on.
    ///
    /// The point is the pairing with [`DANGEROUS_TWIN`] below: a corpus that
    /// merely produces no findings proves nothing, because a rule that matches
    /// nothing at all passes it. This file therefore contains, deliberately,
    /// the *correct* spelling of each shape — parking ops in a `go` body and a
    /// blocking op inside `thread`, a `contains?` over a set and over a vector
    /// index, and each reference kind with its own operators — and
    /// `a_realistic_correct_namespace_produces_no_findings` asserts the
    /// denominators are non-zero as well as that the findings are.
    const CORRECT_CORPUS: &str = r#"(ns app.pipeline
  "Fans work out to a pool and collects the results."
  (:require [clojure.core.async :refer [go go-loop chan <! >! <!! >!! thread close!]]
            [clojure.string :as str]))

(def ^:private known-stages #{:parse :validate :emit})

(defonce ^:private metrics (atom {}))

(defn- record! [stage ms]
  (swap! metrics update stage (fnil conj []) ms))

(defn stage-known? [stage]
  (contains? known-stages stage))

(defn indexed? [v i]
  (contains? v i))

(defn configured? [m]
  (and (contains? m :retries)
       (contains? (:opts m) :timeout-ms)))

(defn produce [out items]
  (go
    (doseq [item items]
      (>! out item))
    (close! out)))

(defn consume [in]
  (go-loop [seen []]
    (if-some [item (<! in)]
      (recur (conj seen item))
      seen)))

(defn drain-blocking [in]
  (loop [seen []]
    (if-some [item (<!! in)]
      (recur (conj seen item))
      seen)))

(defn offload [in out]
  (go
    (let [item (<! in)]
      (thread
        (>!! out (expensive item))))))

(defn render-all [forms]
  (binding [*print-length* 20]
    (mapv pr-str forms)))

(defn render-eagerly [forms]
  (binding [*print-length* 20]
    (->> forms
         (map pr-str)
         (into []))))

(defn with-mocked [f]
  (with-redefs [now (constantly 0)]
    (doall (map f (range 3)))))

(defn tally [items]
  (let [total (atom 0)
        seen (volatile! 0)]
    (doseq [item items]
      (swap! total + (:amount item))
      (vswap! seen inc))
    {:total @total :seen @seen}))

(defn transfer [from to amount]
  (let [source (ref from)
        target (ref to)]
    (dosync
      (alter source - amount)
      (alter target + amount)
      (ref-set source (max 0 @source)))
    [@source @target]))

(defn summarise [entries]
  (let [labels (->> entries
                    (remove (comp str/blank? :id))
                    (map :id)
                    (into []))]
    (str/join ", " labels)))
"#;

    /// The same shapes, each written the way this package reports. Exactly one
    /// occurrence of each rule, so a rule that stopped firing shows up as a
    /// missing name rather than as a smaller number.
    const DANGEROUS_TWIN: &str = r#"(ns app.broken
  (:require [clojure.core.async :refer [go go-loop chan <! >! <!! >!!]]))

(defn pump [in out]
  (go
    (>!! out (inc (<! in)))))

(defn fan-out [chs v]
  (go-loop []
    (run! (fn [c] (>! c v)) chs)
    (recur)))

(defn known? [m id]
  (contains? (keys m) id))

(defn tick [n]
  (let [counter (ref 0)]
    (swap! counter + n)))
"#;

    /// Correct code, every head matched, zero findings — **and** denominators
    /// that prove the heads were matched.
    ///
    /// The denominator half is the part that cannot be faked: without it, a
    /// rule whose head list was misspelled, or whose dialect scope was wrong,
    /// would pass this test by never running.
    #[test]
    fn a_realistic_correct_namespace_produces_no_findings() {
        assert_eq!(
            fired(CORRECT_CORPUS, Dialect::Clojure),
            Vec::<&str>::new(),
            "correct Clojure must be silent"
        );

        let tree =
            SyntaxTree::parse_with_dialect(CORRECT_CORPUS, Dialect::Clojure).expect("parse corpus");
        let path = Path::new("corpus.clj");

        let blocking =
            crate::go_block_blocking_channel_op::domain::build_go_block_blocking_channel_op_report(
                path,
                Dialect::Clojure,
                &tree,
            )
            .expect("report");
        assert_eq!(
            blocking.summary,
            vec![("go_block_count", serde_json::json!(3))]
        );

        let parking = crate::parking_op_outside_go_machinery::domain::build_parking_op_outside_go_machinery_report(
            path,
            Dialect::Clojure,
            &tree,
        )
        .expect("report");
        assert_eq!(
            parking.summary,
            vec![("go_block_count", serde_json::json!(3))]
        );

        let contains =
            crate::contains_on_non_associative::domain::build_contains_on_non_associative_report(
                path,
                Dialect::Clojure,
                &tree,
            )
            .expect("report");
        assert_eq!(
            contains.summary,
            vec![("contains_count", serde_json::json!(4))]
        );

        let references = crate::reference_type_operator_mismatch::domain::build_reference_type_operator_mismatch_report(
            path,
            Dialect::Clojure,
            &tree,
        )
        .expect("report");
        assert_eq!(
            references.summary,
            vec![("reference_binding_count", serde_json::json!(4))]
        );
    }

    /// The twin: each rule fires exactly once, through the real dispatcher.
    #[test]
    fn the_dangerous_twin_fires_every_rule_exactly_once() {
        let mut expected: Vec<&str> = TRIGGERS.iter().map(|(rule, _)| *rule).collect();
        expected.sort_unstable();
        assert_eq!(fired(DANGEROUS_TWIN, Dialect::Clojure), expected);

        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree =
            SyntaxTree::parse_with_dialect(DANGEROUS_TWIN, Dialect::Clojure).expect("parse twin");
        let outcomes = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("twin.clj"),
            Dialect::Clojure,
            &tree,
            DANGEROUS_TWIN,
            RuleSelection::All,
        )
        .expect("lint pass");
        assert_eq!(
            outcomes.len(),
            4,
            "each rule must fire once, not zero or twice"
        );
    }

    /// What the `clean/forms/*` benchmarks measure, in this package's dialect:
    /// ordinary code with none of these heads must not reach a single `check`
    /// body.
    #[test]
    fn a_file_with_none_of_these_heads_produces_no_findings() {
        let clojure = "(ns app.core)\n\
             (def state (atom 0))\n\
             (defn add [a b] (+ a b))\n\
             (reset! state 1)\n\
             (deref state)\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());
    }

    // -- (c) quoting and dialect scope -----------------------------------------

    /// The dispatcher hands a rule every head-matched node, quoted data
    /// included; each `check` calls `support::is_unevaluated_at` to decline
    /// those. Nothing on the report path exercises that call.
    #[test]
    fn no_rule_fires_on_quoted_or_templated_code() {
        for (rule, source) in TRIGGERS {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{rule}: {source} is quoted data"
            );
            assert_eq!(
                fired(&format!("`{source}"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{rule}: {source} is a macro template"
            );
            assert_eq!(
                fired(&format!("(defmacro m [] `(do {source}))"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{rule}: {source} is inside a macro template"
            );
            assert_eq!(
                fired(&format!("(comment {source})"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{rule}: {source} is in a comment block"
            );
        }
    }

    /// Every rule here is `CLOJURE_ONLY`, and this is the assertion that the
    /// declaration is present rather than merely that a Clojure sample happens
    /// to fire.
    #[test]
    fn every_rule_declares_the_clojure_scope_its_vocabulary_belongs_to() {
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            assert_eq!(
                entry.rule().dialect_scope(),
                RuleDialectScope::CLOJURE_ONLY,
                "{} declares the wrong dialect scope",
                entry.meta().name().as_str()
            );
        }
    }

    /// The other half of the scope: bytes the Common Lisp reader accepts, and
    /// which head-match under Common Lisp, must reach no finding.
    ///
    /// `let`, `loop` and `go` are all Common Lisp operators — `go` is the
    /// `tagbody` transfer, which takes a *tag*, not a body — so this package's
    /// head list overlaps Common Lisp far more than its sibling's does. The
    /// scope is the only thing keeping these rules out of them.
    #[test]
    fn the_rules_are_silent_on_the_same_bytes_read_as_common_lisp() {
        for source in [
            "(let ((r (ref 0))) (swap! r #'1+))",
            "(loop (go retry))",
            "(tagbody retry (go retry))",
            "(binding ((*x* 1)) (map #'f xs))",
            "(contains? (keys m) :id)",
        ] {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{source} must be silent as Common Lisp"
            );
        }
    }

    #[test]
    fn no_rule_runs_in_an_unrelated_dialect() {
        for (_, source) in TRIGGERS {
            for dialect in [Dialect::Scheme, Dialect::Racket, Dialect::EmacsLisp] {
                assert_eq!(fired(source, dialect), Vec::<&str>::new());
            }
        }
    }

    // -- (d) no rule declares anything but Heads -------------------------------

    /// `AllNodes` and `WholeTree` are paid for on every file whether or not a
    /// rule matches, which is exactly what the zero-finding benchmarks
    /// measure.
    #[test]
    fn every_rule_declares_a_non_empty_heads_filter() {
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            let name = entry.meta().name().as_str();
            let HeadFilter::Heads(heads) = entry.rule().head_filter() else {
                panic!("{name} declares something other than HeadFilter::Heads");
            };
            assert!(!heads.is_empty(), "{name} declares an empty head list");
        }
    }

    /// Every rule's `Heads` list is **exactly** its domain's head constant.
    ///
    /// The gap this closes is the one a sibling package found by mutation
    /// testing rather than by review: deleting a head from a `HeadFilter` left
    /// its whole suite green, because every domain test calls
    /// `build_…_report`, which walks the tree itself and never consults the
    /// dispatcher's head index. The rule would have silently stopped seeing
    /// that head in production while nothing failed.
    ///
    /// Equality against the domain constant is the assertion that scales: an
    /// eighth binding form, or a third IOC-block macro, has one place to be
    /// added and this test names the other.
    #[test]
    fn every_rules_head_filter_is_exactly_its_domains_head_list() {
        fn heads_of(rule: &dyn paredit_core_lint_engine::rule::LintRule) -> Vec<&'static str> {
            let HeadFilter::Heads(heads) = rule.head_filter() else {
                panic!("not a Heads filter");
            };
            heads.iter().map(|head| head.as_str()).collect()
        }

        assert_eq!(
            heads_of(&crate::contains_on_non_associative::rule::RULE),
            crate::contains_on_non_associative::domain::CONTAINS_HEADS,
        );
        assert_eq!(
            heads_of(&crate::go_block_blocking_channel_op::rule::RULE),
            crate::go_block_blocking_channel_op::domain::GO_BLOCK_HEADS,
        );
        assert_eq!(
            heads_of(&crate::parking_op_outside_go_machinery::rule::RULE),
            crate::parking_op_outside_go_machinery::domain::GO_BLOCK_HEADS,
        );
        assert_eq!(
            heads_of(&crate::reference_type_operator_mismatch::rule::RULE),
            crate::reference_type_operator_mismatch::domain::REFERENCE_BINDING_HEADS,
        );
    }

    // -- (e) cost --------------------------------------------------------------

    /// A timed pass over a generated file, at two sizes.
    ///
    /// Not a benchmark — the machine this runs on is shared, so an absolute
    /// number means nothing. What it pins is the **ratio**: doubling the file
    /// must roughly double the time, because every rule here is linear in the
    /// document. A quadratic rule (the classic being a per-candidate
    /// `tree.root_view()`, or a cross-form correlation that rescans the top
    /// level) shows up as a ratio near four, and the budget below is loose
    /// enough that only an asymptotic regression can trip it.
    ///
    /// The generated form deliberately contains a `let` with a reference
    /// constructor and a `go` block, so the two rules whose per-head-match
    /// work is a *subtree walk* are the ones being measured.
    #[test]
    fn doubling_the_file_does_not_more_than_triple_the_cost() {
        fn elapsed_nanos(forms: usize) -> u128 {
            let source: String = (0..forms)
                .map(|index| {
                    format!(
                        "(defn f{index} [m coll in out]\n  \
                         (let [total (atom 0) seen (volatile! 0)]\n    \
                         (go (doseq [x coll] (>! out x)) (<! in))\n    \
                         (binding [*print-length* 5] (mapv pr-str coll))\n    \
                         (when (contains? m :k) (swap! total inc) (vswap! seen inc))\n    \
                         [@total @seen]))\n"
                    )
                })
                .collect();
            let catalog = RuleCatalog::new(&ENTRIES);
            let index = build_head_index(catalog);
            let tree =
                SyntaxTree::parse_with_dialect(&source, Dialect::Clojure).expect("parse generated");
            let started = std::time::Instant::now();
            let outcome = collect_lint_pass(
                catalog,
                &index,
                Path::new("gen.clj"),
                Dialect::Clojure,
                &tree,
                &source,
                RuleSelection::All,
                PassOptions {
                    settings: None,
                    measure: true,
                },
            )
            .expect("lint pass");
            let total = started.elapsed().as_nanos().max(1);
            assert!(outcome.outcomes.is_empty(), "the generated file is clean");
            assert!(outcome.timings.is_some(), "measure: true records timings");
            total
        }

        let small = elapsed_nanos(500);
        let large = elapsed_nanos(1000);
        // 6x rather than 2x: this runs under a loaded shared sandbox alongside
        // other test binaries, and the point is to catch T×T, which is 4x on a
        // quiet machine and far more on a busy one.
        assert!(
            large < small.saturating_mul(6),
            "doubling the file cost {large}ns against {small}ns for half of it; \
             a rule has gone superlinear"
        );
    }
}
