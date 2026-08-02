//! `disabled-test-left-in` detection: a committed test that can never run,
//! because something in the source turns it off unconditionally.
//!
//! A disabled test is worse than a deleted one: it still reads as coverage, it
//! still appears in the file, and nothing about a green run says it did not
//! execute. This rule is about finding the ones that were switched off "for
//! now" and never switched back.
//!
//! # What this attempts
//!
//! Only spellings a framework actually gives meaning to, read out of the
//! frameworks' own source:
//!
//! - **Clojure**: `^:kaocha/skip` or `^:kaocha/pending` metadata on the test.
//!   `:kaocha/skip` is the shipped default of Kaocha's
//!   `:kaocha.filter/skip-meta`; `:kaocha/pending` is hardcoded in
//!   `kaocha.testable/run-testable`.
//! - **Emacs Lisp**: `(ert-skip …)`, `(skip-unless nil)` or `(skip-when t)` as
//!   a *direct* body form of an `ert-deftest`. The latter two are `cl-macrolet`
//!   bindings ERT injects around the body, which is why they mean nothing
//!   anywhere else.
//! - **Common Lisp**: `(skip …)` as a direct body form, which is FiveAM's
//!   `(defmacro skip (&rest reason))`.
//!
//! # What this does not attempt
//!
//! - **A conditional skip.** `(skip-unless (executable-find "git"))` is a test
//!   that runs wherever it can, which is the opposite of a disabled one. Only a
//!   skip with a literal condition, or none at all, counts — see
//!   [`crate::support::unconditional_skip`].
//! - **A skip nested inside the body.** `(when slow-machine-p (ert-skip "…"))`
//!   is conditional by construction, so only direct body forms are read.
//! - **`:disabled t`.** Asked for, and **not implemented**: no framework here
//!   spells it. FiveAM's `def-test` accepts exactly `:depends-on`, `:suite`,
//!   `:fixture`, `:compile-at` and `:profile`, and passing `:disabled` is a
//!   destructuring error rather than a disabled test.
//! - **`:expected-result :failed`.** An ERT test marked that way still runs;
//!   it is a known-failing test, not a disabled one.
//! - **`^:skip` or `^:pending` on their own.** Bare metadata keys mean whatever
//!   a project's `project.clj` or Kaocha config says they mean, including
//!   nothing. Guessing would put this rule's findings at the mercy of a file it
//!   cannot see.
//! - **A commented-out test.** A comment is not in the tree.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::list_head;
use serde_json::{Value, json};

use crate::support::{
    TEST_DIALECTS, clojure_skip_metadata, for_each_evaluated_subview, normalized_symbol,
    read_test_form, unconditional_skip,
};

#[derive(Debug, Clone)]
pub struct DisabledTestLeftInItem {
    /// The span of the whole test definition, which is what a reader has to
    /// decide about: run it again, or delete it.
    pub span: ByteSpan,
    /// The test's name.
    pub test_name: String,
    /// The marker that disables it, as the report's distinguishing column.
    pub marker: String,
}

impl Finding for DisabledTestLeftInItem {
    fn kind(&self) -> &'static str {
        "disabled-test-left-in"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("test={}", self.test_name),
            format!("marker={}", self.marker),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("test", json!(self.test_name)),
            ("marker", json!(self.marker)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "test {} is disabled in place by {}; run it again or delete it",
            self.test_name, self.marker
        )
    }
}

/// Examines one node. The caller guarantees `view` is evaluated code.
pub fn examine_test(
    view: &ExpressionView,
    dialect: Dialect,
    test_form_count: &mut usize,
    violations: &mut Vec<DisabledTestLeftInItem>,
) {
    let Some(form) = read_test_form(view, dialect) else {
        return;
    };
    let Some(test_name) = form.name_text() else {
        return;
    };
    *test_form_count += 1;

    // Clojure says it in metadata on the definition.
    if let Some(metadata) = clojure_skip_metadata(view, dialect) {
        let marker = normalized_symbol(metadata).unwrap_or_else(|| "skip metadata".to_owned());
        violations.push(DisabledTestLeftInItem {
            span: view.span,
            test_name,
            marker,
        });
        return;
    }

    // Common Lisp and Emacs Lisp say it with a body form. Direct body forms
    // only: anything deeper is inside a conditional and so is conditional.
    for body_form in form.body {
        if let Some(marker) = unconditional_skip(body_form, dialect) {
            violations.push(DisabledTestLeftInItem {
                span: view.span,
                test_name,
                marker: marker.to_owned(),
            });
            return;
        }
    }
}

/// Collects every disabled test in one file, with the number of readable test
/// definitions beside them.
pub fn build_disabled_test_left_in_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DisabledTestLeftInItem>> {
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

    fn findings(input: &str, dialect: Dialect) -> Vec<DisabledTestLeftInItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse input");
        build_disabled_test_left_in_report(Path::new("test.lisp"), dialect, &tree)
            .expect("build report")
            .findings
    }

    fn markers(input: &str, dialect: Dialect) -> Vec<String> {
        findings(input, dialect)
            .into_iter()
            .map(|item| item.marker)
            .collect()
    }

    // -- positives -----------------------------------------------------------

    #[test]
    fn flags_a_kaocha_skipped_clojure_test() {
        assert_eq!(
            markers("(deftest ^:kaocha/skip adds (is true))", Dialect::Clojure),
            vec![":kaocha/skip"]
        );
    }

    #[test]
    fn flags_a_kaocha_pending_clojure_test() {
        assert_eq!(
            markers(
                "(deftest ^:kaocha/pending adds (is true))",
                Dialect::Clojure
            ),
            vec![":kaocha/pending"]
        );
    }

    #[test]
    fn flags_an_ert_test_skipped_outright() {
        assert_eq!(
            markers(
                "(ert-deftest adds () (ert-skip \"broken since 2024\") (should t))",
                Dialect::EmacsLisp
            ),
            vec!["ert-skip"]
        );
    }

    #[test]
    fn flags_an_ert_test_whose_skip_condition_is_a_literal() {
        assert_eq!(
            markers(
                "(ert-deftest adds () (skip-unless nil) (should t))",
                Dialect::EmacsLisp
            ),
            vec!["skip-unless nil"]
        );
        assert_eq!(
            markers(
                "(ert-deftest adds () (skip-when t) (should t))",
                Dialect::EmacsLisp
            ),
            vec!["skip-when t"]
        );
    }

    #[test]
    fn flags_a_five_am_test_that_skips_itself() {
        assert_eq!(
            markers(
                "(def-test adds () (skip \"flaky\") (is t))",
                Dialect::CommonLisp
            ),
            vec!["skip"]
        );
    }

    #[test]
    fn reports_a_disabled_test_once_however_many_markers_it_carries() {
        assert_eq!(
            markers(
                "(ert-deftest adds () (ert-skip \"a\") (ert-skip \"b\"))",
                Dialect::EmacsLisp
            )
            .len(),
            1
        );
    }

    // -- near misses ---------------------------------------------------------

    /// The guard that keeps a portability skip out of this report: a test that
    /// runs wherever it can is not a disabled test.
    #[test]
    fn a_conditional_skip_is_not_a_disabled_test() {
        assert!(
            markers(
                "(ert-deftest adds () (skip-unless (executable-find \"git\")) (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
        assert!(
            markers(
                "(ert-deftest adds () (skip-when (eq system-type 'windows-nt)) (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// `(skip-unless t)` and `(skip-when nil)` both always run.
    #[test]
    fn an_inverted_literal_condition_disables_nothing() {
        assert!(
            markers(
                "(ert-deftest adds () (skip-unless t) (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
        assert!(
            markers(
                "(ert-deftest adds () (skip-when nil) (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// A skip reached only through a conditional is conditional.
    #[test]
    fn a_skip_nested_inside_the_body_is_not_a_direct_body_form() {
        assert!(
            markers(
                "(ert-deftest adds () (when slow-p (ert-skip \"slow\")) (should t))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// Asked for and deliberately not implemented: no framework here spells it.
    #[test]
    fn a_disabled_keyword_option_is_not_a_marker_any_framework_defines() {
        assert!(markers("(def-test adds (:disabled t) (is t))", Dialect::CommonLisp).is_empty());
        assert!(markers("(deftest adds :disabled t (+ 1 2) 3)", Dialect::CommonLisp).is_empty());
    }

    /// A known-failing test still runs.
    #[test]
    fn an_expected_failure_is_not_a_disabled_test() {
        assert!(
            markers(
                "(ert-deftest adds () :expected-result :failed (should (= 1 2)))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    /// A bare metadata key means whatever a project's config says, so it is
    /// not read as a framework marker.
    #[test]
    fn a_metadata_key_with_no_framework_meaning_is_not_a_marker() {
        assert!(markers("(deftest ^:skip adds (is true))", Dialect::Clojure).is_empty());
        assert!(markers("(deftest ^:pending adds (is true))", Dialect::Clojure).is_empty());
        assert!(markers("(deftest ^:integration adds (is true))", Dialect::Clojure).is_empty());
    }

    #[test]
    fn a_skip_spelling_from_another_dialect_is_not_recognized() {
        // `skip-unless` is an ERT macrolet binding, not a Common Lisp form.
        assert!(
            markers(
                "(def-test adds () (skip-unless nil) (is t))",
                Dialect::CommonLisp
            )
            .is_empty()
        );
        // FiveAM's `skip` is not an ERT form.
        assert!(markers("(ert-deftest adds () (skip \"x\"))", Dialect::EmacsLisp).is_empty());
        // Kaocha metadata means nothing in Emacs Lisp.
        assert!(markers("(ert-deftest ^:kaocha/skip adds ())", Dialect::EmacsLisp).is_empty());
    }

    #[test]
    fn an_ordinary_test_is_silent() {
        assert!(markers("(deftest adds (is (= 3 (+ 1 2))))", Dialect::Clojure).is_empty());
        assert!(markers("(ert-deftest adds () (should (= 3 3)))", Dialect::EmacsLisp).is_empty());
        assert!(markers("(def-test adds () (is (= 3 3)))", Dialect::CommonLisp).is_empty());
    }

    /// A skip outside a test definition is some other function's business.
    #[test]
    fn a_skip_outside_a_test_is_not_this_rules_subject() {
        assert!(markers("(defun helper () (skip \"x\"))", Dialect::CommonLisp).is_empty());
    }

    // -- quote and string negatives ------------------------------------------

    #[test]
    fn a_quoted_test_form_is_data_and_is_not_flagged() {
        assert!(markers("'(deftest ^:kaocha/skip adds (is true))", Dialect::Clojure).is_empty());
        assert!(
            markers(
                "(quote (deftest ^:kaocha/skip adds (is true)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Written in Common Lisp on purpose: `,` is an unquote there and plain
    /// whitespace in Clojure.
    #[test]
    fn a_comma_inside_a_hard_quote_does_not_make_a_marker_code_again() {
        assert!(markers("'(a ,(def-test adds () (skip \"x\")))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_quasiquoted_macro_template_is_data() {
        assert!(
            markers(
                "(defmacro m [n] `(deftest ^:kaocha/skip ~n (is true)))",
                Dialect::Clojure
            )
            .is_empty()
        );
    }

    /// Clojure spells unquote `~`; a `,` there is whitespace.
    #[test]
    fn an_unquoted_disabled_test_inside_a_quasiquote_is_code() {
        assert_eq!(
            markers(
                "`(a ~(deftest ^:kaocha/skip adds (is true)))",
                Dialect::Clojure
            ),
            vec![":kaocha/skip"]
        );
        assert_eq!(
            markers("`(a ,(def-test adds () (skip \"x\")))", Dialect::CommonLisp),
            vec!["skip"]
        );
    }

    #[test]
    fn a_marker_spelled_inside_a_string_is_not_a_marker() {
        assert!(
            markers(
                "(ert-deftest adds () (should (equal \"(ert-skip)\" s)))",
                Dialect::EmacsLisp
            )
            .is_empty()
        );
    }

    // -- report envelope -----------------------------------------------------

    #[test]
    fn an_unmodelled_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(deftest adds (skip))", Dialect::Scheme)
            .expect("parse");
        let report = build_disabled_test_left_in_report(Path::new("a.scm"), Dialect::Scheme, &tree)
            .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_denominator_counts_every_readable_test() {
        let tree = SyntaxTree::parse_with_dialect(
            "(deftest a (is true))\n(deftest ^:kaocha/skip b (is true))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_disabled_test_left_in_report(Path::new("t.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        assert_eq!(report.summary, vec![("test_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_names_its_test_and_its_marker() {
        let tree = SyntaxTree::parse_with_dialect(
            "(ns app)\n(deftest ^:kaocha/skip adds\n  (is true))\n",
            Dialect::Clojure,
        )
        .expect("parse");
        let report =
            build_disabled_test_left_in_report(Path::new("t.clj"), Dialect::Clojure, &tree)
                .expect("build report");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "disabled-test-left-in");
        assert_eq!(
            finding.json_fields(),
            vec![("test", json!("adds")), ("marker", json!(":kaocha/skip")),]
        );
    }
}
