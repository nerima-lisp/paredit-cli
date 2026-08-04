#![doc = include_str!("../README.md")]

pub mod accessor_arity;
pub mod append_list_to_cons;
pub mod append_nil;
pub mod car_nthcdr;
pub mod car_reverse;
pub mod cons_to_list;
pub mod destructive_literal;
pub mod double_reverse;
pub mod eql_search_literal;
pub mod hash_table_iteration_order_assumed;
pub mod last_default_count;
pub mod list_star_nil;
pub mod list_star_to_cons;
pub mod nested_get_chain;
pub mod nth_constant_index;
pub mod nthcdr_small_index;
pub mod nthcdr_zero;
pub mod redundant_count_nil;
pub mod redundant_end_nil;
pub mod redundant_eql_test;
pub mod redundant_from_end_nil;
pub mod redundant_identity_key;
pub mod redundant_into_empty_collection;
pub mod redundant_start_zero;
pub mod set_membership_via_linear_scan;
pub mod single_operand_list_op;
pub mod subseq_zero;
pub mod support;

#[cfg(test)]
mod quote_guard_tests;

#[cfg(test)]
mod reader_prefix_fix_tests;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// The four newer rules driven through the *real* dispatcher, rather than by
/// calling `examine` directly.
///
/// A rule's own tests call its `examine`, which cannot answer two questions the
/// engine decides: whether the head index actually routes a node to it, and
/// whether its `check` suppresses what it should. A rule can pass every unit
/// test in its module and still be unreachable — a head the index never
/// produces, or a `dialect_scope` that excludes the only dialect it models.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    pub(crate) static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(
            &crate::hash_table_iteration_order_assumed::rule::META,
            &crate::hash_table_iteration_order_assumed::rule::RULE,
        ),
        RuleEntry::new(
            &crate::nested_get_chain::rule::META,
            &crate::nested_get_chain::rule::RULE,
        ),
        RuleEntry::new(
            &crate::redundant_into_empty_collection::rule::META,
            &crate::redundant_into_empty_collection::rule::RULE,
        ),
        RuleEntry::new(
            &crate::set_membership_via_linear_scan::rule::META,
            &crate::set_membership_via_linear_scan::rule::RULE,
        ),
    ];

    /// The rule names that fire on `source`, sorted so the assertions do not
    /// depend on registration order.
    pub(crate) fn fired(source: &str, dialect: Dialect) -> Vec<&'static str> {
        let catalog = RuleCatalog::new(&ENTRIES);
        let index = build_head_index(catalog);
        let tree = SyntaxTree::parse_with_dialect(source, dialect).expect("parse");
        let mut names: Vec<&'static str> = collect_lint_outcomes(
            catalog,
            &index,
            Path::new("t.lisp"),
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
        names
    }

    // -- each rule reaches the engine ---------------------------------------

    #[test]
    fn every_rule_fires_through_the_real_dispatch() {
        assert_eq!(
            fired(
                "(first (loop for k being the hash-keys of table collect k))",
                Dialect::CommonLisp
            ),
            vec!["hash-table-iteration-order-assumed"]
        );
        assert_eq!(
            fired("(get (get m :a) :b)", Dialect::Clojure),
            vec!["nested-get-chain"]
        );
        assert_eq!(
            fired("(into [] coll)", Dialect::Clojure),
            vec!["redundant-into-empty-collection"]
        );
        assert_eq!(
            fired(
                "(member x '(alpha beta gamma delta epsilon zeta eta theta))",
                Dialect::CommonLisp
            ),
            vec!["set-membership-via-linear-scan"]
        );
    }

    /// A file with none of these rules' heads must cost nothing and report
    /// nothing — the shape the `clean/forms/*` benchmarks measure.
    #[test]
    fn a_file_with_none_of_these_heads_trips_nothing() {
        assert_eq!(
            fired(
                "(defun area (radius)\n  (* pi (expt radius 2)))\n\
                 (defmacro with-log ((name) &body body)\n  `(progn (log ,name) ,@body))\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired(
                "(defn area [radius] (* Math/PI (* radius radius)))",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
    }

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s `is_unevaluated_at` call, every one of these
    /// fires.
    #[test]
    fn no_rule_fires_on_hard_quoted_data() {
        for (source, dialect) in [
            (
                "'(first (loop for k being the hash-keys of table collect k))",
                Dialect::CommonLisp,
            ),
            ("'(first (hash-table-keys table))", Dialect::CommonLisp),
            (
                "'(member x '(alpha beta gamma delta epsilon zeta eta theta))",
                Dialect::CommonLisp,
            ),
            ("'(get (get m :a) :b)", Dialect::Clojure),
            ("'(into [] coll)", Dialect::Clojure),
        ] {
            assert_eq!(fired(source, dialect), Vec::<&str>::new(), "{source}");
        }
    }

    /// A `dialect_scope` that excluded the dialect a rule models would make it
    /// silently unreachable, and no unit test on `examine` could tell.
    #[test]
    fn the_dialect_scopes_gate_the_two_clojure_rules_off_common_lisp() {
        // The Clojure shapes, read as Common Lisp: `get` and `into` are not
        // CLHS operators and these rules must not fire.
        assert_eq!(
            fired("(get (get m :a) :b)", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
        // And the Common Lisp shapes, read as Clojure.
        assert_eq!(
            fired(
                "(first (loop for k being the hash-keys of table collect k))",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired(
                "(member x '(alpha beta gamma delta epsilon zeta eta theta))",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
    }

    /// One expression, one finding: the chain suppression is in `check`, so
    /// only the real dispatch can prove it.
    #[test]
    fn a_three_deep_get_chain_reports_once_through_the_dispatcher() {
        assert_eq!(
            fired("(get (get (get m :a) :b) :c)", Dialect::Clojure),
            vec!["nested-get-chain"]
        );
    }

    /// The threshold is read from the context, so a rule that ignored it would
    /// still pass its own tests.
    #[test]
    fn the_membership_threshold_is_honoured_through_the_dispatcher() {
        assert_eq!(
            fired("(member x '(a b c))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }
}

/// The four newer rules run over realistic, *correct* code — the repo's own
/// Lisp and Clojure fixtures plus a hand-written file per dialect — through the
/// real engine.
///
/// False positives are the failure mode that matters, and a rule's own unit
/// tests cannot find one: they only ever show the rule the shapes its author
/// thought of. This sweep shows it the shapes someone else wrote.
///
/// A zero-findings sweep proves nothing on its own, so each corpus is also
/// asked how many *candidates* it holds — the denominators the report path
/// counts, which is every node whose head the rule matched. A corpus that
/// happens to contain no `car`, no `member` and no `into` would sweep clean
/// while exercising nothing, and `the_corpus_exercises_every_rule` fails if it
/// ever becomes one. The dangerous twins then prove the harness can see a
/// finding at all.
#[cfg(test)]
mod corpus_sweep_tests {
    use std::path::Path;

    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    use super::engine_pass_tests::fired;

    // -- the repo's own fixtures --------------------------------------------

    const CLOJURE_CORPUS: &str = include_str!("../../../../tests/fixtures/corpus/clojure.clj");
    const CLOS_CORPUS: &str = include_str!("../../../../tests/fixtures/corpus/clos.lisp");
    const DEEP_NESTING_CORPUS: &str =
        include_str!("../../../../tests/fixtures/corpus/deep-nesting.lisp");
    const READER_SYNTAX_CORPUS: &str =
        include_str!("../../../../tests/fixtures/corpus/reader-syntax.lisp");
    const GEOMETRY_CORPUS: &str =
        include_str!("../../../../tests/fixtures/semantic_coverage_corpus/geometry.lisp");
    const UTILITIES_CORPUS: &str =
        include_str!("../../../../tests/fixtures/semantic_coverage_corpus/utilities.lisp");

    /// Hand-written Common Lisp that is *correct* and that deliberately walks
    /// up to the edge of all three Common Lisp rules: shallow accessors, short
    /// membership tests, hash iteration whose result is sorted or used
    /// order-blindly, and every near-miss the rules' own guards exist for.
    const REALISTIC_COMMON_LISP: &str = r#"
(defpackage #:inventory
  (:use #:common-lisp)
  (:export #:restock #:summarise))

(in-package #:inventory)

(defparameter *statuses* '(:new :used :refurbished)
  "The three states an item can be in.")

(defun status-known-p (status)
  "A short membership test is exactly what member is for."
  (member status *statuses*))

(defun weekday-p (day)
  (member day '(:mon :tue :wed :thu :fri)))

(defun terminal-status-p (status)
  (member status '(:sold :scrapped)))

(defun label-of (row)
  "Shallow accessors read fine as they stand."
  (list (first row) (second row) (car (cdr row))))

(defun point-coordinates (point)
  (values (first point) (second point) (third point)))

(defun last-entry (rows)
  "The very idiom car-reverse recommends."
  (car (last rows)))

(defun nth-entry (rows index)
  (nth index rows))

(defun head-and-tail (rows)
  (cons (car rows) (cdr rows)))

(defun third-column (row)
  "The ordinal accessor, which is nested-cxr's remedy for a cdr chain."
  (third row))

(defun sorted-keys (table)
  "Sorting is the remedy, so neither of these assumes an order."
  (first (sort (loop for key being the hash-keys of table collect key) #'string<)))

(defun key-count (table)
  (length (loop for key being the hash-keys of table collect key)))

(defun any-key-p (table key)
  (member key (loop for k being the hash-keys of table collect k)))

(defun collected-into-a-sorted-list (table)
  (loop for key being the hash-keys of table
        collect key into keys
        finally (return (first (sort keys #'string<)))))

(defun tally (table)
  "maphash for effect, with no positional read of the result."
  (let ((total 0))
    (maphash (lambda (key value) (declare (ignore key)) (incf total value)) table)
    total))

(defun element-at (sequence index)
  (elt sequence index))

(defun trailing (row)
  (car (nthcdr 4 row)))

(defun rest-of (row)
  (cdr (cdr row)))

(defmacro with-first ((var form) &body body)
  "A quoted template naming the very operators the rules look for."
  `(let ((,var (car ,form)))
     ,@body))

(defun documented-shapes ()
  "Quoted data that must never be read as code."
  '((car (cdr (cdr x)))
    (member x '(alpha beta gamma delta epsilon zeta eta theta))
    (first (loop for k being the hash-keys of table collect k))))
"#;

    /// Hand-written Clojure that is *correct* and walks up to the edge of both
    /// Clojure rules: single lookups, `get-in` already written, `into` with a
    /// transducer or a non-empty target, and the list target whose conversion
    /// would reverse.
    const REALISTIC_CLOJURE: &str = r#"
(ns inventory.core
  (:require [clojure.string :as str]))

(def defaults {:retries 3 :timeout-ms 500})

(defn retries [config]
  (get config :retries))

(defn nested-path [config]
  (get-in config [:http :timeout]))

(defn keyword-access [config]
  (:timeout (:http config)))

(defn threaded [config]
  (-> config :http :timeout))

(defn with-fallback [config]
  (get (get config :http) :timeout :none))

(defn timeout-or [config fallback]
  (get config :timeout fallback))

(defn header [request]
  (get (:headers request) "accept"))

(defn transduced [xs]
  (into [] (map inc) xs))

(defn seeded [xs]
  (into [:header] xs))

(defn accumulated [xs seen]
  (into #{:seen} (remove seen xs)))

(defn reversed [xs]
  "conj onto a list prepends, so this is a reversal and not a conversion."
  (into '() xs))

(defn as-pairs [xs]
  (into {} xs))

(defn already-direct [xs]
  [(vec xs) (set xs) (map str/upper-case xs)])

(defn quoted-template []
  '(into [] (get (get m :a) :b)))
"#;

    /// The same two dialects, written *wrongly* on purpose. If the harness
    /// could not see a finding, every assertion above would pass for the wrong
    /// reason.
    const DANGEROUS_TWIN_COMMON_LISP: &str = r#"
(defun some-key (table)
  (first (loop for key being the hash-keys of table collect key)))

(defun known-p (name)
  (member name '(alpha beta gamma delta epsilon zeta eta theta)))
"#;

    const DANGEROUS_TWIN_CLOJURE: &str = r#"
(ns twin.core)

(defn timeout [config]
  (get (get config :http) :timeout))

(defn as-vector [xs]
  (into [] xs))
"#;

    fn common_lisp_corpora() -> Vec<(&'static str, &'static str)> {
        vec![
            ("clos.lisp", CLOS_CORPUS),
            ("deep-nesting.lisp", DEEP_NESTING_CORPUS),
            ("reader-syntax.lisp", READER_SYNTAX_CORPUS),
            ("geometry.lisp", GEOMETRY_CORPUS),
            ("utilities.lisp", UTILITIES_CORPUS),
            ("hand-written.lisp", REALISTIC_COMMON_LISP),
        ]
    }

    fn clojure_corpora() -> Vec<(&'static str, &'static str)> {
        vec![
            ("clojure.clj", CLOJURE_CORPUS),
            ("hand-written.clj", REALISTIC_CLOJURE),
        ]
    }

    // -- no false positives on correct code ----------------------------------

    #[test]
    fn no_rule_fires_on_realistic_correct_common_lisp() {
        for (name, source) in common_lisp_corpora() {
            assert_eq!(
                fired(source, Dialect::CommonLisp),
                Vec::<&str>::new(),
                "{name} is correct code"
            );
        }
    }

    #[test]
    fn no_rule_fires_on_realistic_correct_clojure() {
        for (name, source) in clojure_corpora() {
            assert_eq!(
                fired(source, Dialect::Clojure),
                Vec::<&str>::new(),
                "{name} is correct code"
            );
        }
    }

    // -- the corpus actually exercises what the sweep claims to cover --------

    /// How many nodes each rule's head filter matched across a corpus — the
    /// denominators the report path counts, which is the number of *candidates*
    /// the rule rejected rather than the number of findings it made.
    fn candidate_counts(sources: &[(&str, &str)], dialect: Dialect) -> (u64, u64, u64, u64) {
        let mut accessors = 0;
        let mut members = 0;
        let mut gets = 0;
        let mut intos = 0;
        for (name, source) in sources {
            let tree = SyntaxTree::parse_with_dialect(source, dialect)
                .unwrap_or_else(|error| panic!("{name} must parse: {error:?}"));
            let path = Path::new(name);
            accessors += summary_of(
                &crate::hash_table_iteration_order_assumed::domain::collect_hash_order_assumptions(
                    path, dialect, &tree,
                )
                .expect("collect")
                .summary,
                "accessor_form_count",
            );
            members += summary_of(
                &crate::set_membership_via_linear_scan::domain::collect_linear_scans(
                    path, dialect, &tree,
                )
                .expect("collect")
                .summary,
                "member_form_count",
            );
            gets += summary_of(
                &crate::nested_get_chain::domain::collect_get_chains(path, dialect, &tree)
                    .expect("collect")
                    .summary,
                "get_form_count",
            );
            intos += summary_of(
                &crate::redundant_into_empty_collection::domain::collect_redundant_intos(
                    path, dialect, &tree,
                )
                .expect("collect")
                .summary,
                "into_form_count",
            );
        }
        (accessors, members, gets, intos)
    }

    fn summary_of(summary: &[(&'static str, serde_json::Value)], key: &str) -> u64 {
        summary
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| value.as_u64())
            .unwrap_or_default()
    }

    /// The trap this exists for: a corpus containing none of what a rule looks
    /// for sweeps clean while proving nothing at all about that rule.
    #[test]
    fn the_corpus_exercises_every_rule() {
        let (accessors, members, ..) =
            candidate_counts(&common_lisp_corpora(), Dialect::CommonLisp);
        assert!(
            accessors >= 15,
            "the Common Lisp corpus must exercise the accessor rule; saw {accessors}"
        );
        assert!(
            members >= 4,
            "the Common Lisp corpus must exercise the membership rule; saw {members}"
        );

        let (.., gets, intos) = candidate_counts(&clojure_corpora(), Dialect::Clojure);
        assert!(
            gets >= 4,
            "the Clojure corpus must exercise the nested-get rule; saw {gets}"
        );
        assert!(
            intos >= 4,
            "the Clojure corpus must exercise the into rule; saw {intos}"
        );
    }

    // -- the dangerous twins -------------------------------------------------

    /// If the harness could not see a finding, every clean-sweep assertion
    /// above would pass for the wrong reason.
    #[test]
    fn the_dangerous_twins_are_caught() {
        let mut common_lisp = fired(DANGEROUS_TWIN_COMMON_LISP, Dialect::CommonLisp);
        common_lisp.dedup();
        assert_eq!(
            common_lisp,
            vec![
                "hash-table-iteration-order-assumed",
                "set-membership-via-linear-scan",
            ]
        );

        let mut clojure = fired(DANGEROUS_TWIN_CLOJURE, Dialect::Clojure);
        clojure.dedup();
        assert_eq!(
            clojure,
            vec!["nested-get-chain", "redundant-into-empty-collection"]
        );
    }

    /// The twins differ from the realistic files only in the defect, so the
    /// corpora above are not clean merely by being unlike the rules' subjects.
    #[test]
    fn the_twins_and_the_realistic_files_share_their_shapes() {
        for (name, source, dialect) in [
            (
                "common lisp",
                DANGEROUS_TWIN_COMMON_LISP,
                Dialect::CommonLisp,
            ),
            ("clojure", DANGEROUS_TWIN_CLOJURE, Dialect::Clojure),
        ] {
            assert!(
                !fired(source, dialect).is_empty(),
                "the {name} twin must be dirty"
            );
        }
    }
}
