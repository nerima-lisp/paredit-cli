#![doc = include_str!("../README.md")]

pub mod disabled_test_left_in;
pub mod duplicate_test_name;
pub mod empty_test_body;
pub mod sleep_in_test;
pub mod support;
pub mod test_asserts_constant;
pub mod test_without_assertion;

// The root's REGISTRY names each rule's META and RULE across this crate
// boundary (section 4.2), and each slice's cli owns its own subcommand.

/// Every rule here driven through the *engine*, rather than through its own
/// `build_*_report`.
///
/// The two entry points do not share their quote handling. A report walks with
/// [`crate::support::for_each_evaluated_subview`], which never visits data at
/// all; a head-filtered rule is handed matched nodes by the dispatcher
/// *including* the ones inside `'(…)`, and depends on each `check`'s
/// [`crate::support::is_unevaluated_at`] call to decline them. Testing only the
/// report would leave that call — the one thing standing between those five
/// rules and a finding on every quoted example in a macro's documentation —
/// unexercised. (`duplicate-test-name` is the exception at both ends: it is
/// `HeadFilter::WholeTree` and *is* its report, so the dispatcher hands it the
/// root and the report's own walk answers the quote question.)
///
/// Running the real pass also covers the two declarations a domain test cannot
/// see: each rule's head filter and its `RuleDialectScope`.
#[cfg(test)]
mod engine_pass_tests {
    use std::path::Path;

    use paredit_core_lint_engine::engine::{build_head_index, collect_lint_outcomes};
    use paredit_core_lint_engine::policy::RuleSelection;
    use paredit_core_lint_engine::rule::{RuleCatalog, RuleEntry};
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    static ENTRIES: [RuleEntry; 6] = [
        RuleEntry::new(
            &crate::disabled_test_left_in::rule::META,
            &crate::disabled_test_left_in::rule::RULE,
        ),
        RuleEntry::new(
            &crate::duplicate_test_name::rule::META,
            &crate::duplicate_test_name::rule::RULE,
        ),
        RuleEntry::new(
            &crate::empty_test_body::rule::META,
            &crate::empty_test_body::rule::RULE,
        ),
        RuleEntry::new(
            &crate::sleep_in_test::rule::META,
            &crate::sleep_in_test::rule::RULE,
        ),
        RuleEntry::new(
            &crate::test_asserts_constant::rule::META,
            &crate::test_asserts_constant::rule::RULE,
        ),
        RuleEntry::new(
            &crate::test_without_assertion::rule::META,
            &crate::test_without_assertion::rule::RULE,
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
            fired("(deftest adds (println 1))", Dialect::Clojure),
            vec!["test-without-assertion"]
        );
        assert_eq!(
            fired("(deftest adds)", Dialect::Clojure),
            vec!["empty-test-body"]
        );
        // A real assertion in the body, so this isolates the skip marker
        // rather than also tripping `test-asserts-constant`.
        assert_eq!(
            fired(
                "(deftest ^:kaocha/skip adds (is (= 1 (f))))",
                Dialect::Clojure
            ),
            vec!["disabled-test-left-in"]
        );
        assert_eq!(
            fired("(deftest adds (is true))", Dialect::Clojure),
            vec!["test-asserts-constant"]
        );
        assert_eq!(
            fired(
                "(deftest waits (Thread/sleep 100) (is (= 1 (f))))",
                Dialect::Clojure
            ),
            vec!["sleep-in-test"]
        );
        assert_eq!(
            fired(
                "(deftest adds (is (= 1 (f))))\n(deftest adds (is (= 2 (f))))\n",
                Dialect::Clojure
            ),
            vec!["duplicate-test-name"]
        );
    }

    // -- the guard the report path cannot exercise ---------------------------

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without each `check`'s `is_unevaluated_at` call, every one of these fires.
    #[test]
    fn no_rule_fires_on_a_hard_quoted_test_form() {
        for source in [
            "'(deftest adds (println 1))",
            "'(deftest adds)",
            "'(deftest ^:kaocha/skip adds (is true))",
            "'(deftest adds (is true))",
            "'(deftest waits (Thread/sleep 100) (is (= 1 (f))))",
        ] {
            assert_eq!(
                fired(source, Dialect::Clojure),
                Vec::<&str>::new(),
                "{source} is quoted data"
            );
        }
    }

    /// `duplicate-test-name` needs two forms to say anything, so the quoted
    /// case above cannot reach it. It is also the one rule with no
    /// `is_unevaluated_at` call — being `HeadFilter::WholeTree`, it relies on
    /// its report's own evaluated-only walk — which is exactly why the engine
    /// pass has to ask.
    #[test]
    fn a_quoted_duplicate_does_not_fire_through_the_real_dispatch() {
        assert_eq!(
            fired(
                "(deftest adds (is (= 1 (f))))\n'(deftest adds (is (= 2 (f))))\n",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn no_rule_fires_inside_a_long_hand_quote_form() {
        assert_eq!(
            fired("(quote (deftest adds (is true)))", Dialect::Clojure),
            Vec::<&str>::new()
        );
    }

    /// A macro template: the `deftest` is built, not defined.
    #[test]
    fn no_rule_fires_inside_a_quasiquoted_macro_template() {
        assert_eq!(
            fired(
                "(defmacro def-case [n] `(deftest ~n (println 1)))",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired(
                "(defmacro def-case (n) `(def-test ,n () (= 1 2)))",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// A comma inside a hard quote is a literal comma, not an escape back to
    /// code — the shape a single depth counter reads wrongly.
    #[test]
    fn no_rule_fires_on_a_comma_inside_a_hard_quote() {
        assert_eq!(
            fired("'(a ,(def-test adds () (= 1 2)))", Dialect::CommonLisp),
            Vec::<&str>::new()
        );
    }

    /// The one shape that *is* code again.
    #[test]
    fn an_unquoted_test_inside_a_quasiquote_still_fires() {
        assert_eq!(
            fired("`(a ,(def-test adds () (= 1 2)))", Dialect::CommonLisp),
            vec!["test-without-assertion"]
        );
    }

    // -- the declarations a domain test cannot see ---------------------------

    /// `RuleDialectScope`: the dispatcher skips a rule before walking anything.
    #[test]
    fn no_rule_runs_outside_its_declared_dialects() {
        assert_eq!(fired("(deftest adds)", Dialect::Scheme), Vec::<&str>::new());
        assert_eq!(fired("(deftest adds)", Dialect::Racket), Vec::<&str>::new());
        assert_eq!(fired("(deftest adds)", Dialect::Fennel), Vec::<&str>::new());
    }

    /// `HeadFilter::Heads`: an ordinary definition is never handed to the five
    /// head-filtered rules, which is what keeps the zero-finding benchmarks
    /// cheap. `duplicate-test-name` is `WholeTree` and does see this file, and
    /// says nothing about it, which is the other half of the same assertion.
    #[test]
    fn no_rule_sees_a_form_that_is_not_a_test_definition() {
        assert_eq!(
            fired(
                "(defn adds [a b] (+ a b))\n(defmacro m [] nil)\n(def x 1)\n",
                Dialect::Clojure
            ),
            Vec::<&str>::new()
        );
        assert_eq!(
            fired(
                "(defun adds (a b) (+ a b))\n(defmethod frob ((x integer)) x)\n",
                Dialect::CommonLisp
            ),
            Vec::<&str>::new()
        );
    }

    /// A realistic, correct test file: the case a reviewer runs first.
    #[test]
    fn a_correct_test_file_produces_no_findings() {
        let clojure = "(ns app.core-test\n  (:require [clojure.test :refer [deftest testing is are]]\n            [app.core :as core]))\n\n\
             (deftest add-test\n  (testing \"adds two numbers\"\n    (is (= 3 (core/add 1 2))))\n  \
             (testing \"is commutative\"\n    (are [a b] (= (core/add a b) (core/add b a))\n      1 2\n      3 4)))\n\n\
             (deftest subtract-test\n  (is (= 1 (core/subtract 3 2)))\n  (is (thrown? IllegalArgumentException (core/subtract nil 1))))\n";
        assert_eq!(fired(clojure, Dialect::Clojure), Vec::<&str>::new());

        let elisp = "(require 'ert)\n\n\
             (ert-deftest app-add-test ()\n  \"Adding two numbers.\"\n  :tags '(:fast)\n  \
             (should (= 3 (app-add 1 2)))\n  (should-not (app-add nil nil))\n  \
             (should-error (app-add \"a\" 1) :type 'wrong-type-argument))\n\n\
             (ert-deftest app-platform-test ()\n  (skip-unless (executable-find \"git\"))\n  \
             (should (app-git-available-p)))\n";
        assert_eq!(fired(elisp, Dialect::EmacsLisp), Vec::<&str>::new());

        let common_lisp = "(in-package :app/test)\n\n\
             (def-test add-test (:suite arithmetic)\n  (is (= 3 (app:add 1 2)))\n  \
             (is-true (app:positive-p 1))\n  (signals type-error (app:add \"a\" 1)))\n\n\
             (deftest rt-add (app:add 1 2) 3)\n\n\
             (define-test lisp-unit-add\n  (assert-equal 3 (app:add 1 2))\n  (assert-error 'type-error (app:add \"a\" 1)))\n";
        assert_eq!(fired(common_lisp, Dialect::CommonLisp), Vec::<&str>::new());
    }

    /// A test whose assertions live in a project helper, and one built by a
    /// macro: the two shapes that most often turn a rule like these into
    /// noise.
    #[test]
    fn a_helper_driven_and_a_macro_generated_test_file_produces_no_findings() {
        let source = "(ns app.helper-test\n  (:require [clojure.test :refer [deftest]]\n            [app.support :refer [verify-invariants]]))\n\n\
             (deftest invariants-test\n  (verify-invariants (app.core/build-state)))\n\n\
             (defmacro defcase [n expected]\n  `(deftest ~n (clojure.test/is (= ~expected (app.core/run)))))\n\n\
             (defcase first-case 1)\n";
        assert_eq!(fired(source, Dialect::Clojure), Vec::<&str>::new());
    }
}
