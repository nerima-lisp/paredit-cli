#![doc = include_str!("../README.md")]

pub mod clojure_pre_post_vacuous;
pub mod support;
pub mod typed_racket_arity_mismatch;

#[cfg(test)]
mod cost_tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2).

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
///   receives a single node in production. This package is unusually exposed to
///   that: `typed-racket-arity-mismatch` is *about* `(: …)` forms but is
///   anchored on `define`, because [`NormalizedHead`] rejects a head containing
///   a colon at compile time.
/// - the [`RuleDialectScope`], which the dispatcher consults *before* walking
///   anything. Two dialects are covered by two rules — one Clojure, one
///   Racket — and `typed-racket-arity-mismatch` is the first built-in rule
///   scoped to Racket **alone**, so a single-dialect Racket scope has no other
///   coverage anywhere. Both rules state their scope explicitly, so that the
///   standalone report and the engine always read one constant.
///
/// [`NormalizedHead`]: paredit_core_lint_engine::model::NormalizedHead
/// [`RuleDialectScope`]: paredit_core_lint_engine::policy::RuleDialectScope
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::model::HeadFilter;
    use paredit_core_lint_engine::policy::{RuleDialectScope, RuleSelection};
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 2] = [
        RuleEntry::new(
            &crate::clojure_pre_post_vacuous::rule::META,
            &crate::clojure_pre_post_vacuous::rule::RULE,
        ),
        RuleEntry::new(
            &crate::typed_racket_arity_mismatch::rule::META,
            &crate::typed_racket_arity_mismatch::rule::RULE,
        ),
    ];

    /// One source per rule that triggers exactly that rule and no other.
    const CLOJURE_TRIGGERS: [(&str, &str); 1] = [(
        "clojure-pre-post-vacuous",
        "(defn withdraw [amount] {:pre [true]} (- balance amount))",
    )];

    const RACKET_TRIGGERS: [(&str, &str); 1] = [(
        "typed-racket-arity-mismatch",
        "(: scale (-> Integer Integer))\n(define (scale factor value) (* factor value))",
    )];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.src"),
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

    /// Like [`fired`], but for a source a given reader rejects outright.
    fn parses_as(source: &str, dialect: Dialect) -> bool {
        SyntaxTree::parse_with_dialect(source, dialect).is_ok()
    }

    // -- (a) each rule is reached through the head index ----------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        for (rule, source) in CLOJURE_TRIGGERS {
            assert_eq!(
                fired(source, Dialect::Clojure),
                vec![rule],
                "{rule} is unreachable through the head index"
            );
        }
        for (rule, source) in RACKET_TRIGGERS {
            assert_eq!(
                fired(source, Dialect::Racket),
                vec![rule],
                "{rule} is unreachable through the head index"
            );
        }
    }

    /// Two rules, two distinct names: a copy-paste in `ENTRIES` that
    /// registered one slice twice would otherwise leave the loop above green.
    #[test]
    fn the_catalog_holds_both_rules_once_each() {
        let mut names: Vec<&'static str> = RuleCatalog::new(&ENTRIES)
            .entries()
            .iter()
            .map(|entry| entry.meta().name().as_str())
            .collect();
        assert_eq!(names.len(), 2);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 2);
    }

    // -- (b) a file with none of these heads trips nothing --------------------

    /// What the `clean/forms/*` benchmarks measure: ordinary code, none of it
    /// carrying a contract, must not reach a single `check` body.
    #[test]
    fn a_file_with_none_of_these_heads_produces_no_findings() {
        let clojure = "(ns app.core)\n\
             (def state (atom 0))\n\
             (let [x 1] (inc x))\n\
             (defmacro unless [test then] `(if ~test nil ~then))\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());

        let racket = "#lang typed/racket\n\
             (require racket/list)\n\
             (provide scale)\n\
             (struct point ([x : Integer] [y : Integer]))\n";
        assert_eq!(fired(racket, Dialect::Racket), Vec::<&str>::new());
    }

    // -- (c) realistic, *correct* code in each dialect -----------------------

    /// Idiomatic Clojure that a reviewer would write by hand, using every shape
    /// this rule comes near: real preconditions, a `%` in `:post` where it
    /// belongs, a `%` inside a `#(…)` literal inside a `:pre`, a multi-arity
    /// `defn`, a docstring and attribute map, and a function that legitimately
    /// returns a map.
    ///
    /// Paired with [`the_dangerous_twin_is_still_detected`], without which a
    /// sweep that silently stopped matching anything would still pass.
    const CORRECT_CLOJURE: &str = r#"(ns bank.account
  (:require [clojure.string :as str]))

(defn withdraw
  "Removes amount from balance."
  {:added "1.2"}
  [balance amount]
  {:pre  [(number? balance) (pos? amount) (>= balance amount)]
   :post [(>= % 0) (number? %)]}
  (- balance amount))

(defn- normalize [xs]
  {:pre [(every? #(pos? %) xs) (seq xs)]}
  (map #(/ % (apply max xs)) xs))

(defn describe
  ([x] (describe x "unknown"))
  ([x fallback]
   {:pre  [(some? fallback)]
    :post [(string? %)]}
   (str (or x fallback))))

(defn config []
  {:pre [true] :timeout 30 :retries 3})

(defn tagged [% ]
  {:pre [(keyword? %)]}
  (name %))
"#;

    /// Idiomatic Typed Racket exercising every arrow shape the arity rule must
    /// decline as well as the ones it must accept.
    const CORRECT_RACKET: &str = r#"#lang typed/racket

(provide scale describe sum-all greet)

(: scale (-> Integer Integer Integer))
(define (scale factor value)
  (* factor value))

(: zero (-> Integer))
(define (zero) 0)

(: sum-all (-> Integer * Integer))
(define (sum-all . values)
  (apply + values))

(: greet (-> String #:loud Boolean String))
(define (greet who #:loud loud)
  (if loud (string-upcase who) who))

(: describe (->* (String) (Integer) String))
(define (describe subject [times 1])
  (string-append subject (number->string times)))

(: pick (case-> (-> Integer) (-> Integer Integer)))
(define (pick [n 0]) n)

(: identity-of (All (A) (-> A A)))
(define (identity-of x) x)

(: infix-style (Integer -> Boolean))
(define (infix-style n) (> n 0))

(: curried (-> Integer (-> Integer Integer)))
(define ((curried a) b) (+ a b))

(: is-zero? (-> Number #:equality (-> Number Number Any) [#:zero Number] Any))
(define (is-zero? n #:equality equality #:zero [zero 0])
  (equality n zero))

(: string-or-symbol? (-> Any Boolean : (U String Symbol)))
(define (string-or-symbol? x)
  (or (string? x) (symbol? x)))

(: tvars (-> Integer Integer))
(define #:forall (A) (tvars n) n)
"#;

    #[test]
    fn realistic_correct_clojure_produces_no_findings() {
        assert_eq!(
            fired(CORRECT_CLOJURE, Dialect::Clojure),
            Vec::<&str>::new(),
            "a false positive on idiomatic Clojure"
        );
    }

    #[test]
    fn realistic_correct_racket_produces_no_findings() {
        assert_eq!(
            fired(CORRECT_RACKET, Dialect::Racket),
            Vec::<&str>::new(),
            "a false positive on idiomatic Typed Racket"
        );
    }

    /// The control for the two sweeps above: a sweep that had silently stopped
    /// matching anything would pass them both. Each twin is the *correct* file
    /// with exactly one thing made wrong, and each proves one detector still
    /// works on it.
    ///
    /// One twin per rule rather than one combined twin. A combined one is easy
    /// to get wrong — `[true (>= % 0)]` is not vacuous, because `[true]` is
    /// only a no-op when *every* element is the literal `true` — and a twin
    /// that tested for two findings while producing one would have to be
    /// weakened to pass, quietly costing the control its value.
    #[test]
    fn the_dangerous_twin_is_still_detected() {
        // (1) Real preconditions replaced by a vacuous one.
        let vacuous = CORRECT_CLOJURE.replace(
            "{:pre  [(number? balance) (pos? amount) (>= balance amount)]\n   :post [(>= % 0) (number? %)]}",
            "{:pre  [true]}",
        );
        assert_ne!(vacuous, CORRECT_CLOJURE, "the twin must actually differ");
        assert_eq!(
            fired(&vacuous, Dialect::Clojure),
            vec!["clojure-pre-post-vacuous"]
        );

        // (2) One argument dropped from a correct two-argument annotation.
        let racket = CORRECT_RACKET.replace(
            "(: scale (-> Integer Integer Integer))",
            "(: scale (-> Integer Integer))",
        );
        assert_ne!(racket, CORRECT_RACKET, "the twin must actually differ");
        assert_eq!(
            fired(&racket, Dialect::Racket),
            vec!["typed-racket-arity-mismatch"]
        );
    }

    // -- (d) quoted and templated code ---------------------------------------

    /// The dispatcher hands a rule every head-matched node, quoted data
    /// included; each `check` calls the package's `is_unevaluated_at` to
    /// decline those. Note the unquote spellings differ: Clojure's is `~`, and
    /// Racket's is `,`.
    #[test]
    fn no_rule_fires_on_quoted_or_templated_code() {
        for (_, source) in CLOJURE_TRIGGERS {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
            assert_eq!(
                fired(&format!("(defmacro m [] `{source})"), Dialect::Clojure),
                Vec::<&str>::new(),
                "{source} is a macro template"
            );
        }
        for (_, source) in RACKET_TRIGGERS {
            assert_eq!(
                fired(&format!("'{source}"), Dialect::Racket),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    // -- (e) each rule fires in its own dialect and is silent in the others ---

    /// `typed-racket-arity-mismatch` is the built-in suite's first
    /// Racket-scoped rule, so this is the only coverage that arm of the
    /// dispatch has. The bytes are chosen to parse in all four dialects — no
    /// `#t`, no `[`, no `{` — so the scope is the *only* difference between
    /// the runs. Scheme is the sharpest control: it shares Racket's entire
    /// surface syntax here.
    #[test]
    fn the_racket_only_rule_fires_on_racket_and_on_nothing_else() {
        let dual = "(: scale (-> Integer Integer))\n(define (scale factor value) 1)\n";
        assert_eq!(
            fired(dual, Dialect::Racket),
            vec!["typed-racket-arity-mismatch"]
        );
        for dialect in [
            Dialect::Scheme,
            Dialect::CommonLisp,
            Dialect::Clojure,
            Dialect::EmacsLisp,
        ] {
            assert!(
                parses_as(dual, dialect),
                "{dialect:?} must read these bytes"
            );
            assert_eq!(fired(dual, dialect), Vec::<&str>::new(), "{dialect:?}");
        }
    }

    /// The Clojure rule, the other way round. It has no dual-syntax source: it
    /// needs a `[…]` parameter vector and a `{…}` condition map, and neither
    /// the Common Lisp nor the Scheme reader accepts those. So its silence
    /// elsewhere is pinned two ways — the Clojure spelling is not readable at
    /// all as those dialects, and every reader that *does* accept the bytes
    /// reaches no finding.
    #[test]
    fn the_clojure_only_rule_fires_on_clojure_and_on_nothing_else() {
        for (rule, source) in CLOJURE_TRIGGERS {
            assert_eq!(fired(source, Dialect::Clojure), vec![rule]);
            assert!(
                !parses_as(source, Dialect::CommonLisp),
                "{rule}'s trigger must not even read as Common Lisp"
            );
            // Whichever of the remaining readers accept these bytes must still
            // find nothing; the ones that reject them cannot report either.
            for dialect in [
                Dialect::Racket,
                Dialect::Scheme,
                Dialect::EmacsLisp,
                Dialect::Fennel,
            ] {
                if parses_as(source, dialect) {
                    assert_eq!(fired(source, dialect), Vec::<&str>::new(), "{dialect:?}");
                }
            }
        }
        // At least one non-Clojure reader really does accept the bytes, so the
        // loop above is not vacuously true.
        assert!(
            CLOJURE_TRIGGERS
                .iter()
                .any(|(_, source)| parses_as(source, Dialect::Racket)),
            "no non-Clojure reader accepts any trigger; the scope is untested"
        );
    }

    /// No rule here is meaningful in a dialect with neither vocabulary: Clojure
    /// condition maps or Typed Racket annotations.
    #[test]
    fn no_rule_runs_in_an_unrelated_dialect() {
        for (_, source) in RACKET_TRIGGERS {
            for dialect in [Dialect::Fennel, Dialect::Janet, Dialect::Hy] {
                assert_eq!(fired(source, dialect), Vec::<&str>::new(), "{dialect:?}");
            }
        }
    }

    /// The scopes as declarations, so a slice that loses its `dialect_scope`
    /// override fails here and not only through a source sample that might stop
    /// triggering for some other reason.
    #[test]
    fn each_rule_declares_the_scope_its_vocabulary_belongs_to() {
        let expected_of = |name: &str| match name {
            // The suite's first Racket-scoped rule: there is no `RACKET_ONLY`
            // constant, so the scope is constructed explicitly.
            "typed-racket-arity-mismatch" => RuleDialectScope::new(&[Dialect::Racket]),
            _ => RuleDialectScope::CLOJURE_ONLY,
        };
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            let name = entry.meta().name().as_str();
            assert_eq!(
                entry.rule().dialect_scope(),
                expected_of(name),
                "{name} declares the wrong dialect scope"
            );
        }
    }

    /// **No** rule here is Common Lisp. Stated separately because Common Lisp
    /// is the [`RuleDialectScope`] *trait default*: a rule that silently lost
    /// its `dialect_scope` override would fall back to Common Lisp and start
    /// walking every `.lisp` file in a repository, and nothing in the loop
    /// above distinguishes "declared Common Lisp" from "defaulted to it".
    ///
    /// This package used to ship one deliberately Common-Lisp-scoped rule,
    /// `check-type-redundant-with-declare`, and dropped it; see the README for
    /// why it is not coming back. Its removal is what turns this assertion from
    /// "exactly one" into "none", which is the stronger guard.
    #[test]
    fn no_rule_runs_for_common_lisp() {
        let common_lisp: Vec<&str> = RuleCatalog::new(&ENTRIES)
            .entries()
            .iter()
            .filter(|entry| entry.rule().dialect_scope().includes(Dialect::CommonLisp))
            .map(|entry| entry.meta().name().as_str())
            .collect();
        assert_eq!(common_lisp, Vec::<&str>::new());
    }

    /// Every rule declares exactly one dialect, and between them they cover
    /// two. A rule that widened its scope to "everything" would still pass
    /// the per-rule assertions if they were rewritten to match it.
    #[test]
    fn the_two_dialects_are_covered_and_no_rule_claims_more_than_one() {
        for dialect in [Dialect::Racket, Dialect::Clojure] {
            assert!(
                RuleCatalog::new(&ENTRIES)
                    .entries()
                    .iter()
                    .any(|entry| entry.rule().dialect_scope().includes(dialect)),
                "no rule covers {dialect:?}"
            );
        }
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            let claimed = Dialect::ALL
                .iter()
                .filter(|dialect| entry.rule().dialect_scope().includes(**dialect))
                .count();
            assert_eq!(
                claimed,
                1,
                "{} claims {claimed} dialects; each rule here models exactly one",
                entry.meta().name().as_str()
            );
        }
    }

    // -- (f) no rule declares anything but Heads ------------------------------

    /// `AllNodes` and `WholeTree` are paid for on every file whether or not a
    /// rule matches, which is exactly what the zero-finding benchmarks measure.
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

    /// Every rule here is report-only, and every one is a `Warning`: a declared
    /// contract that says the wrong thing is worth reporting, and never worth
    /// rewriting on the author's behalf.
    #[test]
    fn every_rule_is_report_only() {
        use paredit_core_lint_engine::model::Fixability;
        for entry in RuleCatalog::new(&ENTRIES).entries() {
            assert_eq!(
                entry.meta().fixability(),
                Fixability::ReportOnly,
                "{}",
                entry.meta().name().as_str()
            );
        }
    }
}
