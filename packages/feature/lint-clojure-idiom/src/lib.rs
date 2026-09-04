#![doc = include_str!("../README.md")]

pub mod apply_with_literal_collection;
pub mod def_inside_function_body;
pub mod single_key_nested_path;
pub mod support;
pub mod with_open_returns_lazy_seq;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// Two declarations decide whether a rule is reachable from the CLI at all, and
/// neither is visible to a domain test, which calls `examine_*` on a node it
/// picked itself:
///
/// - the `HeadFilter::Heads` list, which is what the dispatcher's head index is
///   built from. A head spelled wrongly — or a head the domain matches but the
///   list omits — leaves every `examine_*` test green while the rule never
///   receives a single node in production.
/// - the [`RuleDialectScope`], which the dispatcher consults *before* walking
///   anything. Every rule here is `CLOJURE_ONLY`, so a rule that lost its
///   override would silently start running over Common Lisp with a Clojure
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
            &crate::apply_with_literal_collection::rule::META,
            &crate::apply_with_literal_collection::rule::RULE,
        ),
        RuleEntry::new(
            &crate::def_inside_function_body::rule::META,
            &crate::def_inside_function_body::rule::RULE,
        ),
        RuleEntry::new(
            &crate::single_key_nested_path::rule::META,
            &crate::single_key_nested_path::rule::RULE,
        ),
        RuleEntry::new(
            &crate::with_open_returns_lazy_seq::rule::META,
            &crate::with_open_returns_lazy_seq::rule::RULE,
        ),
    ];

    /// A Clojure source that triggers exactly one of these rules, per rule.
    ///
    /// Each is written to miss the other three, which is not automatic: a
    /// `with-open` trigger spelled `(with-open [r (io/reader p)] (map parse
    /// (line-seq r)))` must not also contain a one-key `get-in`.
    const TRIGGERS: [(&str, &str); 4] = [
        ("apply-with-literal-collection", "(apply str [\"a\" \"b\"])"),
        (
            "def-inside-function-body",
            "(defn load! [p] (def config (read-edn p)) config)",
        ),
        ("single-key-nested-path", "(get-in m [:k])"),
        (
            "with-open-returns-lazy-seq",
            "(with-open [r (io/reader p)] (map parse (line-seq r)))",
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
    /// the *correct* spelling of each shape — an eager `->>` out of
    /// `with-open`, a two-key `get-in`, an `apply` over a computed sequence, a
    /// `let` where a `def` would be wrong, and `empty?`/`seq` where a `count`
    /// comparison would be — and the test asserts the denominators are non-zero
    /// as well as that the findings are.
    const CORRECT_CORPUS: &str = r#"(ns app.ingest
  "Reads a ledger file and summarises it."
  (:require [clojure.java.io :as io]
            [clojure.string :as str]))

(def ^:private defaults
  {:retries 3
   :tags #{:ingest :ledger}})

(defonce cache (atom {}))

(defn- parse-line [line]
  (let [[id amount] (str/split line #",")]
    {:id id :amount (parse-long amount)}))

(defn read-entries
  "Realises every entry before the reader closes."
  [path]
  (with-open [r (io/reader path)]
    (->> (line-seq r)
         (remove str/blank?)
         (map parse-line)
         (into []))))

(defn tally [path]
  (with-open [r (io/reader path)]
    (reduce + 0 (map :amount (map parse-line (line-seq r))))))

(defn summarise [entries]
  (if (empty? entries)
    {:total 0 :ids []}
    {:total (reduce + (map :amount entries))
     :ids (mapv :id entries)}))

(defn describe [entries]
  (when (seq entries)
    (apply str (interpose ", " (map :id entries)))))

(defn bump [m id]
  (-> m
      (update-in [:hits id] (fnil inc 0))
      (assoc-in [:last :id] id)
      (assoc :touched? true)))

(defn lookup [m id]
  (or (get-in m [:hits id])
      (get m :default)))

(defn retry [f attempts]
  (loop [attempt 1]
    (let [result (f)]
      (if (or (some? result) (>= attempt attempts))
        result
        (recur (inc attempt))))))

(defn settled? [entry]
  (and (= (:status entry) :settled)
       (zero? (:outstanding entry))
       (pos? (:amount entry))
       (> (:amount entry) 0)
       (not= (:id entry) "")))

(defn largest [entries]
  (when-not (empty? entries)
    (apply max (map :amount entries))))

(defn render [m]
  (str "total=" (:total m) " n=" (count (:ids m))))
"#;

    /// The same shapes, each written the way this package reports. Exactly one
    /// occurrence of each rule, so a rule that stopped firing shows up as a
    /// missing name rather than as a smaller number.
    const DANGEROUS_TWIN: &str = r#"(ns app.broken
  (:require [clojure.java.io :as io]))

(defn read-entries [path]
  (with-open [r (io/reader path)]
    (map parse-line (line-seq r))))

(defn load-config! [path]
  (def config (read-edn path))
  config)

(defn greeting [a b]
  (apply str ["hello " a b]))

(defn lookup [m id]
  (get-in m [id]))
"#;

    /// Correct code, every head matched, zero findings — **and** a denominator
    /// that proves the heads were matched.
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

        let with_open =
            crate::with_open_returns_lazy_seq::domain::build_with_open_returns_lazy_seq_report(
                path,
                Dialect::Clojure,
                &tree,
            )
            .expect("report");
        assert_eq!(
            with_open.summary,
            vec![("resource_scope_count", serde_json::json!(2))]
        );

        let defs = crate::def_inside_function_body::domain::build_def_inside_function_body_report(
            path,
            Dialect::Clojure,
            &tree,
        )
        .expect("report");
        assert_eq!(
            defs.summary,
            vec![("function_definition_count", serde_json::json!(11))]
        );

        let applies =
            crate::apply_with_literal_collection::domain::build_apply_with_literal_collection_report(
                path,
                Dialect::Clojure,
                &tree,
            )
            .expect("report");
        assert_eq!(applies.summary, vec![("apply_count", serde_json::json!(2))]);

        let paths = crate::single_key_nested_path::domain::build_single_key_nested_path_report(
            path,
            Dialect::Clojure,
            &tree,
        )
        .expect("report");
        assert_eq!(
            paths.summary,
            vec![("nested_path_count", serde_json::json!(3))]
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

    /// The repo's own Clojure layout fixture, embedded rather than read from
    /// `tests/fixtures/` so this package's tests do not depend on a path
    /// outside it. It is idiomatic correct Clojure and must be silent.
    const REPO_CORPUS_EXCERPT: &str = r#"(ns paredit.corpus
  (:require [clojure.set :as set]
            [clojure.string :as str]))

(def ^:private default-options
  {:retries 3 :timeout-ms 500 :tags #{:corpus :layout}})

(defn classify [n]
  (cond (neg? n) :negative (zero? n) :zero (> n 100) :large :else :positive))

(defn transfer [{:keys [balance] :as from} to amount]
  (when (< balance amount)
    (throw (ex-info "insufficient funds" {:account (:id from) :amount amount})))
  [(update from :balance - amount) (update to :balance + amount)])

(defn summarise [accounts]
  (let [total (reduce + (map :balance accounts))
        labels (->> accounts
                    (remove (comp zero? :balance))
                    (map #(str (:id %) ":" (:balance %))))]
    (str/join ", " (conj (vec labels) (str "total " total)))))

(defn report [items]
  (doseq [item items :when (:active? item)]
    (println (:name item) "->" (render item))))

(defn tags-in-common [a b]
  (set/intersection (:tags a #{}) (:tags b #{})))
"#;

    #[test]
    fn the_repositorys_own_clojure_fixture_is_silent() {
        assert_eq!(
            fired(REPO_CORPUS_EXCERPT, Dialect::Clojure),
            Vec::<&str>::new()
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
    /// `(apply str '("a" "b"))` and `(get-in m '(:k))` both read as Common
    /// Lisp, and the first even means roughly the same thing there — `apply` is
    /// a CLHS operator. The scope is the only thing keeping this rule set out
    /// of them.
    #[test]
    fn the_rules_are_silent_on_the_same_bytes_read_as_common_lisp() {
        for source in [
            "(apply str '(\"a\" \"b\"))",
            "(get-in m '(:k))",
            "(defun f () (defvar *x* 1))",
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
    /// rule matches, which is exactly what the zero-finding benchmarks measure.
    /// The README states this as a package-wide property; this is the test that
    /// makes it one.
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
    /// The gap this closes was found by mutation testing, not by review:
    /// deleting `defmethod` from `def-inside-function-body`'s `HeadFilter`
    /// left the whole suite green. The domain test
    /// `flags_a_def_in_a_defmethod_body` still passed, because it calls
    /// `build_…_report`, which walks the tree itself and never consults the
    /// dispatcher's head index — and `TRIGGERS` above happens to spell that
    /// rule's trigger as a `defn`. So the rule would have silently stopped
    /// seeing every `defmethod` in production while nothing failed.
    ///
    /// Equality against the domain constant is the assertion that scales: a
    /// fourth nested-path operator, or a fourth definer, has one place to be
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
            heads_of(&crate::apply_with_literal_collection::rule::RULE),
            crate::apply_with_literal_collection::domain::APPLY_HEADS,
        );
        assert_eq!(
            heads_of(&crate::def_inside_function_body::rule::RULE),
            crate::def_inside_function_body::domain::FUNCTION_DEFINITION_HEADS,
        );
        assert_eq!(
            heads_of(&crate::single_key_nested_path::rule::RULE),
            crate::single_key_nested_path::domain::NESTED_PATH_HEADS,
        );
        assert_eq!(
            heads_of(&crate::with_open_returns_lazy_seq::rule::RULE),
            crate::with_open_returns_lazy_seq::domain::RESOURCE_SCOPE_HEADS,
        );
    }

    /// A timed pass over a generated file, at two sizes.
    ///
    /// Not a benchmark — the machine this runs on is shared, so an absolute
    /// number means nothing. What it pins is the **ratio**: doubling the file
    /// must roughly double the time, because every rule here is linear in the
    /// document. A quadratic rule (the classic being a per-candidate
    /// `tree.root_view()`) shows up as a ratio near four, and the budget below
    /// is loose enough that only an asymptotic regression can trip it.
    #[test]
    fn doubling_the_file_does_not_more_than_triple_the_cost() {
        fn elapsed_nanos(forms: usize) -> u128 {
            let source: String = (0..forms)
                .map(|index| {
                    format!(
                        "(defn f{index} [m coll]\n  (let [n (count coll)]\n    \
                         (when (seq coll)\n      (apply str (map str coll))\n      \
                         (get-in m [:a :b])\n      (= n 1))))\n"
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
