//! `test-without-assertion` detection: a test definition whose body runs code
//! but never asserts anything, so it passes as long as it does not crash.
//!
//! # What this attempts
//!
//! Exactly one shape, and it is deliberately a narrow one: a test whose body
//! contains no assertion form **and** every call in that body is one this
//! module already knows cannot assert. The second half is the whole design.
//! Without it, `(deftest t1 (check-invariants x))` — a test whose assertion
//! lives one function call away — would be reported, and that is the single
//! most common way a correct test is written.
//!
//! So an unrecognized call is read as "this might assert" and the test is left
//! alone. That trades every finding on a test that delegates to a helper for
//! never accusing one, which is the right way round: a missed vacuous test
//! costs nothing, and a wrongly accused correct test costs a reader's trust in
//! the whole rule.
//!
//! # What this does not attempt
//!
//! - **`rt`'s `(deftest name form &rest expected-values)`.** Its assertion *is*
//!   the positional comparison of the form's values against the expected ones;
//!   it contains no assertion form by design, and reporting it would flag every
//!   `rt` test in a file. [`crate::support::TestKind::body_is_assertions`] is
//!   what excludes it.
//! - **`defspec`.** Its property is generated, not asserted, for the same
//!   reason.
//! - **FiveAM's `(test …)`.** Modelling a head that generic is not worth the
//!   tests it would wrongly claim; see [`crate::support::TestKind`].
//! - **An empty body.** That is `empty-test-body`'s finding, and reporting it
//!   twice under two names helps nobody.
//! - **A test built by a macro.** A `deftest` inside a quasiquoted template is
//!   data, and the assertion it will have comes from the macro's caller.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, calls_any, contains_assertion, for_each_evaluated_subview, read_test_form,
};

#[derive(Debug, Clone)]
pub struct TestWithoutAssertionItem {
    /// The span of the whole test definition.
    pub span: ByteSpan,
    /// The test's name, as the report's one distinguishing column.
    pub test_name: String,
}

impl Finding for TestWithoutAssertionItem {
    fn kind(&self) -> &'static str {
        "test-without-assertion"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("test={}", self.test_name)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("test", json!(self.test_name))]
    }

    fn message(&self) -> String {
        format!(
            "test {} contains no assertion; it passes whenever it does not signal",
            self.test_name
        )
    }
}

/// The calls this module is willing to conclude cannot assert.
///
/// Not a list of "things a test may contain" — a list of things whose presence
/// leaves the verdict unchanged. Anything absent from it stops the analysis,
/// so the cost of an omission is a missed finding and never a wrong one. It is
/// therefore kept to binding and control forms, the standard sequence and
/// arithmetic operators, and the printing and sleeping calls whose presence in
/// an assertion-free test is the very thing worth reporting.
const fn non_asserting_heads(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &[
            "let",
            "let*",
            "progn",
            "prog1",
            "prog2",
            "when",
            "unless",
            "if",
            "cond",
            "case",
            "dolist",
            "dotimes",
            "loop",
            "block",
            "return",
            "return-from",
            "setq",
            "setf",
            "incf",
            "decf",
            "push",
            "pop",
            "and",
            "or",
            "not",
            "null",
            "list",
            "list*",
            "cons",
            "append",
            "vector",
            "values",
            "quote",
            "function",
            "lambda",
            "format",
            "print",
            "princ",
            "prin1",
            "pprint",
            "write-line",
            "write-string",
            "terpri",
            "sleep",
            "+",
            "-",
            "*",
            "/",
            "1+",
            "1-",
            "=",
            "/=",
            "<",
            ">",
            "<=",
            ">=",
            "eq",
            "eql",
            "equal",
            "equalp",
            "string=",
            "length",
            "car",
            "cdr",
            "first",
            "second",
            "rest",
            "nth",
            "elt",
            "aref",
            "gethash",
            "funcall",
            "apply",
            "identity",
        ],
        Dialect::EmacsLisp => &[
            "let",
            "let*",
            "progn",
            "prog1",
            "when",
            "unless",
            "if",
            "cond",
            "dolist",
            "dotimes",
            "while",
            "setq",
            "setf",
            "push",
            "pop",
            "and",
            "or",
            "not",
            "null",
            "list",
            "cons",
            "append",
            "vector",
            "quote",
            "function",
            "lambda",
            "message",
            "format",
            "format-message",
            "print",
            "princ",
            "prin1",
            "insert",
            "sleep-for",
            "sit-for",
            "ignore",
            "+",
            "-",
            "*",
            "/",
            "=",
            "/=",
            "<",
            ">",
            "<=",
            ">=",
            "eq",
            "eql",
            "equal",
            "string=",
            "string-equal",
            "length",
            "car",
            "cdr",
            "nth",
            "elt",
            "aref",
            "gethash",
            "funcall",
            "apply",
            "identity",
        ],
        Dialect::Clojure => &[
            "let", "do", "if", "if-not", "when", "when-not", "cond", "condp", "case", "doseq",
            "dotimes", "loop", "recur", "testing", "println", "print", "prn", "pr", "printf",
            "str", "format", "list", "vector", "vec", "conj", "cons", "assoc", "dissoc", "get",
            "get-in", "count", "first", "rest", "next", "nth", "seq", "map", "filter", "reduce",
            "into", "not", "and", "or", "=", "not=", "<", ">", "<=", ">=", "+", "-", "*", "/",
            "inc", "dec", "quote", "fn", "identity",
        ],
        _ => &[],
    }
}

/// Whether every call reachable as code in `body` is one
/// [`non_asserting_heads`] lists.
///
/// An atom is not a call and never blocks the verdict; a `(…)` list whose head
/// is not a bare symbol (`((f) x)`, `([1 2] 0)`) is unreadable and does block
/// it.
fn every_call_is_known_non_asserting(body: &[ExpressionView], dialect: Dialect) -> bool {
    let known = non_asserting_heads(dialect);
    let mut all_known = true;
    for form in body {
        for_each_evaluated_subview(form, |view| {
            if view.children.is_empty() {
                return;
            }
            if !calls_any(view, known) {
                all_known = false;
            }
        });
        if !all_known {
            return false;
        }
    }
    true
}

/// Examines one node, which the rule reaches through the single dispatch pass
/// and the report through its own evaluated-code walk.
///
/// The caller guarantees `view` is evaluated code; neither entry point calls
/// this on quoted data.
pub fn examine_test(
    view: &ExpressionView,
    dialect: Dialect,
    test_form_count: &mut usize,
    violations: &mut Vec<TestWithoutAssertionItem>,
) {
    let Some(form) = read_test_form(view, dialect) else {
        return;
    };
    // `rt`'s positional `deftest` and `defspec` assert without an assertion
    // form; asking whether they contain one is a question with no meaning.
    if !form.kind.body_is_assertions() {
        return;
    }
    let Some(test_name) = form.name_text() else {
        return;
    };
    *test_form_count += 1;

    // An empty body is `empty-test-body`'s subject.
    if form.body.is_empty() {
        return;
    }
    if contains_assertion(form.body, dialect) {
        return;
    }
    // The guard that keeps a test whose assertion lives inside a helper out of
    // this report.
    if !every_call_is_known_non_asserting(form.body, dialect) {
        return;
    }

    violations.push(TestWithoutAssertionItem {
        span: view.span,
        test_name,
    });
}

/// Collects every assertion-free test in one file, with the number of test
/// definitions this rule can speak about beside them.
///
/// The denominator counts only the tests whose body is a sequence of assertion
/// forms — an `rt` `deftest` is not one, and including it would make the ratio
/// answer a question the rule never asked.
pub fn build_test_without_assertion_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TestWithoutAssertionItem>> {
    let modelled = TEST_DIALECTS.contains(&dialect);
    let mut test_form_count = 0;
    let mut violations = Vec::new();

    if modelled {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            if list_head(view).is_some() {
                examine_test(view, dialect, &mut test_form_count, &mut violations);
            }
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        modelled,
        tree.source(),
        violations,
        vec![("test_form_count", json!(test_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(input: &str, dialect: Dialect) -> Vec<TestWithoutAssertionItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_test_without_assertion_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
    }

    fn names(input: &str, dialect: Dialect) -> Vec<String> {
        findings(input, dialect)
            .into_iter()
            .map(|item| item.test_name)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_clojure_test_that_only_prints() {
        assert_eq!(
            names("(deftest adds (println (+ 1 2)))", Dialect::Clojure),
            vec!["adds"]
        );
    }

    #[test]
    fn flags_an_ert_test_that_only_sets_a_variable() {
        assert_eq!(
            names("(ert-deftest adds () (setq x (+ 1 2)))", Dialect::EmacsLisp),
            vec!["adds"]
        );
    }

    #[test]
    fn flags_a_five_am_test_whose_body_is_a_bare_comparison() {
        // `(= 1 2)` computes a boolean and throws it away; FiveAM needs `is`.
        assert_eq!(
            names("(def-test adds () (= 1 2))", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    #[test]
    fn flags_a_lisp_unit_test_with_no_assert_form() {
        assert_eq!(
            names("(define-test adds (+ 1 2))", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    // -- near misses ---------------------------------------------------------

    #[test]
    fn a_test_with_an_assertion_is_silent() {
        assert!(names("(deftest adds (is (= 3 (+ 1 2))))", Dialect::Clojure).is_empty());
        assert!(names("(ert-deftest adds () (should (= 3 3)))", Dialect::EmacsLisp).is_empty());
        assert!(names("(def-test adds () (is (= 3 3)))", Dialect::CommonLisp).is_empty());
        assert!(names("(define-test adds (assert-equal 3 3))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn an_assertion_nested_below_a_grouping_form_still_counts() {
        assert!(
            names(
                "(deftest adds (testing \"sums\" (let [x 1] (is (= x 1)))))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// The guard the whole rule turns on: an assertion one call away.
    #[test]
    fn a_test_delegating_to_a_helper_is_silent() {
        assert!(names("(deftest adds (check-invariants 1 2))", Dialect::Clojure).is_empty());
        assert!(
            names(
                "(ert-deftest adds () (my-project--assert-sum 1 2))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// The same guard, one level down: the unknown call is not the outermost
    /// form.
    #[test]
    fn a_helper_call_nested_inside_a_known_form_still_silences_the_rule() {
        assert!(
            names(
                "(deftest adds (let [x 1] (verify-total x)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// A framework this package does not model: every call in the body is
    /// unknown, so nothing is concluded.
    #[test]
    fn a_test_using_an_unmodelled_framework_is_silent() {
        // Midje, whose `fact` is not a modelled definition head at all.
        assert!(names("(fact \"adds\" (+ 1 2) => 3)", Dialect::Clojure).is_empty());
        // A modelled head, but an unmodelled assertion vocabulary inside it.
        assert!(names("(deftest adds (expect 3 (+ 1 2)))", Dialect::Clojure).is_empty());
    }

    /// `rt`'s `deftest` asserts by comparing values positionally, with no
    /// assertion form anywhere. Flagging it would flag every `rt` test.
    #[test]
    fn an_rt_style_common_lisp_deftest_is_never_flagged() {
        assert!(names("(deftest adds (+ 1 2) 3)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_defspec_is_never_flagged() {
        assert!(
            names(
                "(defspec adds 100 (prop/for-all [x gen/int] (= x x)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    #[test]
    fn an_empty_body_is_left_to_the_empty_test_body_rule() {
        assert!(names("(deftest adds)", Dialect::Clojure).is_empty());
        assert!(names("(ert-deftest adds ())", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn a_let_bound_test_is_read_like_any_other() {
        // The `deftest` is nested, not top-level, and still has its assertion.
        assert!(
            names(
                "(let [expected 3] (deftest adds (is (= expected 3))))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_test_form_is_data_and_is_not_flagged() {
        assert!(names("'(deftest adds (println 1))", Dialect::Clojure).is_empty());
        assert!(names("(quote (deftest adds (println 1)))", Dialect::Clojure).is_empty());
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure, so this shape only means anything in a dialect
    /// that reads the comma.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_a_test_code_again() {
        assert!(names("'(a ,(def-test adds () (= 1 2)))", Dialect::CommonLisp).is_empty());
    }

    /// A macro template: the test is built, not defined, and its assertion
    /// comes from the caller.
    #[test]
    fn a_quasiquoted_macro_template_is_data() {
        assert!(
            names(
                "(defmacro def-case [n] `(deftest ~n (println 1)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// The one shape a quasiquote does *not* protect: an unquote escapes back
    /// to code, so a test spliced in through `~` is a real definition. Clojure
    /// spells unquote `~`; a `,` there is whitespace.
    #[test]
    fn an_unquoted_test_inside_a_quasiquote_is_code() {
        assert_eq!(
            names("`(a ~(deftest adds (println 1)))", Dialect::Clojure),
            vec!["adds"]
        );
        assert_eq!(
            names("`(a ,(def-test adds () (= 1 2)))", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    #[test]
    fn a_test_form_spelled_inside_a_string_is_not_a_test() {
        assert!(names("(println \"(deftest adds (println 1))\")", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_string_body_does_not_hide_an_assertion_from_the_walk() {
        // The `is` here is text, not a form, so the test really does assert
        // nothing and is correctly reported.
        assert_eq!(
            names(
                "(deftest adds (println \"(is (= 1 1))\"))",
                Dialect::Clojure
            ),
            vec!["adds"]
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(deftest adds (display 1))", Dialect::Scheme)
            .expect("parse");
        let report =
            build_test_without_assertion_report(Path::new("a.scm"), Dialect::Scheme, &tree)
                .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_denominator_counts_only_the_tests_this_rule_can_speak_about() {
        let tree = SyntaxTree::parse_with_dialect(
            "(deftest a (+ 1 2) 3)\n(def-test b () (is t))\n(define-test c (assert-true t))\n",
            Dialect::CommonLisp,
        )
        .expect("parse");
        let report =
            build_test_without_assertion_report(Path::new("t.lisp"), Dialect::CommonLisp, &tree)
                .expect("build report");
        // The `rt` deftest is excluded; the other two are counted.
        assert_eq!(report.summary, vec![("test_form_count", json!(2))]);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_test_name() {
        let tree = SyntaxTree::parse_with_dialect(
            "(ns app.core-test)\n(deftest adds\n  (println 1))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_test_without_assertion_report(
            Path::new("core_test.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "test-without-assertion");
        assert_eq!(finding.json_fields(), vec![("test", json!("adds"))]);
        assert_eq!(finding.text_columns(), vec!["test=adds".to_owned()]);
    }
}
