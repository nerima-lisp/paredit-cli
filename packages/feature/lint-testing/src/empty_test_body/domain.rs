//! `empty-test-body` detection: a test definition with nothing in it, which
//! every framework here reports as a pass.
//!
//! # What this attempts
//!
//! One shape: a readable test definition whose body slice is empty. Where the
//! body starts is [`crate::support::read_test_form`]'s answer, and a form whose
//! shape it cannot read produces no finding at all — an `ert-deftest` with no
//! argument list, a test whose name is a designator list, a name computed by a
//! macro.
//!
//! # What this does not attempt
//!
//! - **A test that is deliberately pending.** A Clojure test carrying
//!   `^:kaocha/skip` or `^:kaocha/pending` with no body is a placeholder its
//!   author wrote on purpose; Kaocha reports it as pending rather than running
//!   it. Reporting that as a mistake is exactly the false positive this rule
//!   would otherwise be worth nothing for.
//! - **An Emacs Lisp test whose only child is a docstring.**
//!   `(ert-deftest f () "doc")` reads that string as the body, deliberately,
//!   so this rule never claims it is empty.
//! - **A body of `nil`, `()` or a comment.** `(deftest f nil)` has a body form,
//!   and a comment is not in the tree. Neither is the shape this rule names.
//! - **Deciding what should go in the body.** `Fixability::ReportOnly`, because
//!   the only honest fix is a test someone still has to write.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, clojure_skip_metadata, for_each_evaluated_subview, read_test_form,
};

#[derive(Debug, Clone)]
pub struct EmptyTestBodyItem {
    /// The span of the whole test definition.
    pub span: ByteSpan,
    /// The test's name.
    pub test_name: String,
}

impl Finding for EmptyTestBodyItem {
    fn kind(&self) -> &'static str {
        "empty-test-body"
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
            "test {} has an empty body; it is reported as a pass having checked nothing",
            self.test_name
        )
    }
}

/// Examines one node. The caller guarantees `view` is evaluated code.
pub fn examine_test(
    view: &ExpressionView,
    dialect: Dialect,
    test_form_count: &mut usize,
    violations: &mut Vec<EmptyTestBodyItem>,
) {
    let Some(form) = read_test_form(view, dialect) else {
        return;
    };
    let Some(test_name) = form.name_text() else {
        return;
    };
    *test_form_count += 1;

    if !form.body.is_empty() {
        return;
    }
    // A pending test is empty on purpose.
    if clojure_skip_metadata(view, dialect).is_some() {
        return;
    }

    violations.push(EmptyTestBodyItem {
        span: view.span,
        test_name,
    });
}

/// Collects every empty test body in one file, with the number of readable test
/// definitions beside them.
pub fn build_empty_test_body_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EmptyTestBodyItem>> {
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

    fn names(input: &str, dialect: Dialect) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_empty_test_body_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
            .into_iter()
            .map(|item| item.test_name)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_an_empty_test_in_each_modelled_dialect() {
        assert_eq!(names("(deftest adds)", Dialect::Clojure), vec!["adds"]);
        assert_eq!(
            names("(ert-deftest adds ())", Dialect::EmacsLisp),
            vec!["adds"]
        );
        assert_eq!(names("(deftest adds)", Dialect::CommonLisp), vec!["adds"]);
        assert_eq!(
            names("(def-test adds ())", Dialect::CommonLisp),
            vec!["adds"]
        );
        assert_eq!(
            names("(define-test adds)", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    #[test]
    fn flags_an_empty_rt_test_left_with_only_its_keyword_properties() {
        assert_eq!(
            names("(deftest adds :compile-at :run-time)", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    // -- near misses ---------------------------------------------------------

    #[test]
    fn a_test_with_one_body_form_is_silent() {
        assert!(names("(deftest adds (is true))", Dialect::Clojure).is_empty());
        assert!(names("(ert-deftest adds () (should t))", Dialect::EmacsLisp).is_empty());
    }

    /// `nil` is a body form, not the absence of one.
    #[test]
    fn a_body_of_nil_is_a_body() {
        assert!(names("(ert-deftest adds () nil)", Dialect::EmacsLisp).is_empty());
        assert!(names("(deftest adds nil)", Dialect::Clojure).is_empty());
    }

    #[test]
    fn an_ert_test_whose_only_child_is_a_docstring_is_silent() {
        assert!(names("(ert-deftest adds () \"not yet\")", Dialect::EmacsLisp).is_empty());
    }

    /// The guard that keeps a deliberately pending Kaocha test out of this
    /// report.
    #[test]
    fn a_pending_clojure_test_is_empty_on_purpose() {
        assert!(names("(deftest ^:kaocha/pending adds)", Dialect::Clojure).is_empty());
        assert!(names("(deftest ^:kaocha/skip adds)", Dialect::Clojure).is_empty());
    }

    /// A metadata key with no framework meaning does not excuse an empty body.
    #[test]
    fn an_unrelated_metadata_key_does_not_silence_the_rule() {
        assert_eq!(
            names("(deftest ^:integration adds)", Dialect::Clojure),
            vec!["adds"]
        );
    }

    #[test]
    fn a_shape_that_cannot_be_read_produces_nothing() {
        // No argument list: not an `ert-deftest` this package can locate a
        // body in.
        assert!(names("(ert-deftest adds)", Dialect::EmacsLisp).is_empty());
        // A designator list rather than a name.
        assert!(names("(deftest (adds :suite s))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_test_using_an_unmodelled_framework_is_silent() {
        assert!(names("(fact \"adds\")", Dialect::Clojure).is_empty());
        assert!(names("(test adds)", Dialect::CommonLisp).is_empty());
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_test_form_is_data_and_is_not_flagged() {
        assert!(names("'(deftest adds)", Dialect::Clojure).is_empty());
        assert!(names("(quote (deftest adds))", Dialect::Clojure).is_empty());
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_a_test_code_again() {
        assert!(names("'(a ,(def-test adds ()))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_quasiquoted_macro_template_is_data() {
        assert!(names("(defmacro m [n] `(deftest ~n))", Dialect::Clojure).is_empty());
    }

    /// Clojure spells unquote `~`; a `,` there is whitespace.
    #[test]
    fn an_unquoted_test_inside_a_quasiquote_is_code() {
        assert_eq!(
            names("`(a ~(deftest adds))", Dialect::Clojure),
            vec!["adds"]
        );
        assert_eq!(
            names("`(a ,(def-test adds ()))", Dialect::CommonLisp),
            vec!["adds"]
        );
    }

    #[test]
    fn a_test_form_spelled_inside_a_string_is_not_a_test() {
        assert!(names("(println \"(deftest adds)\")", Dialect::Clojure).is_empty());
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(deftest adds)", Dialect::Scheme).expect("parse");
        let report = build_empty_test_body_report(Path::new("a.scm"), Dialect::Scheme, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_readable_test_not_only_the_empty_ones() {
        let tree = SyntaxTree::parse_with_dialect(
            "(deftest a)\n(deftest b (is true))\n(deftest c)\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_empty_test_body_report(Path::new("t.clj"), Dialect::Clojure, &tree)
            .expect("build report");
        assert_eq!(report.summary, vec![("test_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_test_name() {
        let tree = SyntaxTree::parse_with_dialect("(ns app)\n(deftest adds)\n", Dialect::Clojure)
            .expect("parse");
        let report = build_empty_test_body_report(Path::new("t.clj"), Dialect::Clojure, &tree)
            .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "empty-test-body");
        assert_eq!(finding.json_fields(), vec![("test", json!("adds"))]);
    }
}
