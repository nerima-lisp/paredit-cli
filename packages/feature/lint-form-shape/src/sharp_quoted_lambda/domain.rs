//! Common Lisp sharp-quoted-`lambda` detection: a `(lambda …)` form carrying a
//! `#'` (function) reader prefix — `#'(lambda (x) …)`. The `lambda` macro
//! already expands to `(function (lambda …))`, i.e. `#'(lambda …)`, so the
//! explicit `#'` in front is pure duplication: `#'(lambda (x) x)` is exactly
//! `(lambda (x) x)` in every position (argument, value, or binding). Dropping
//! the `#'` is the modern idiom and removes the noise.
//!
//! Only a `(lambda …)` form with exactly the function prefix is flagged. A bare
//! `(lambda …)` is already idiomatic; `#'foo` on a *symbol* is a normal
//! function reference (never redundant); and `#'` on any other form is left
//! alone. Auto-fixable: the fix strips the `#'`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether `view` is a `(lambda …)` list form (ignoring any reader prefix).
fn is_lambda_form(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("lambda"))
}

#[derive(Debug, Clone)]
pub struct SharpQuotedLambdaItem {
    /// The span of the whole `#'(lambda …)` form (prefix included).
    pub span: ByteSpan,
}

impl Finding for SharpQuotedLambdaItem {
    /// Fixed: this rule has exactly one shape to report, and no sub-kind to
    /// separate.
    fn kind(&self) -> &'static str {
        "sharp-quoted-lambda"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Empty, because the old text row carried nothing past the path and
    /// offset: the span alone locates the redundant `#'`.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// Empty, because the old JSON carried only the path and span, which the
    /// envelope already emits.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The same sentence the `sharp-quoted-lambda` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "#' on a lambda is redundant; #'(lambda …) is (lambda …)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_lambda(
    view: &ExpressionView,
    lambda_form_count: &mut usize,
    violations: &mut Vec<SharpQuotedLambdaItem>,
) {
    if !is_lambda_form(view) {
        return;
    }
    *lambda_form_count += 1;

    // Flag only a lambda form whose sole reader prefix is the function `#'`.
    if view.reader_prefixes.len() == 1 && view.reader_prefixes[0] == ReaderPrefix::Function {
        violations.push(SharpQuotedLambdaItem { span: view.span });
    }
}

/// Collects every `#'(lambda …)` in one file, with the number of `lambda` forms
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `#'` here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_sharp_quoted_lambda_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SharpQuotedLambdaItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("lambda_form_count", json!(0))],
        ));
    }

    let mut lambda_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_lambda(subview, &mut lambda_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("lambda_form_count", json!(lambda_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SharpQuotedLambdaItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_sharp_quoted_lambda_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build sharp-quoted lambda report")
    }

    /// The `(lambda_form_count, violations)` pair the report is built from.
    fn lambdas(input: &str) -> (u64, Vec<SharpQuotedLambdaItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "lambda_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("lambda_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_sharp_quoted_lambda() {
        let source = "(mapcar #'(lambda (x) (* x x)) xs)";
        let (count, violations) = lambdas(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].span), "#'(lambda (x) (* x x))");
    }

    #[test]
    fn does_not_flag_a_bare_lambda() {
        let (count, violations) = lambdas("(mapcar (lambda (x) x) xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_sharp_quoted_symbol() {
        // #'foo is a normal function reference.
        let (count, violations) = lambdas("(mapcar #'foo xs)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_lambda_head() {
        let (_, violations) = lambdas("#'(LAMBDA (x) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_a_top_level_sharp_quoted_lambda() {
        let (_, violations) = lambdas("(setf f #'(lambda () 42))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_sharp_quoted_lambda() {
        let (_, violations) = lambdas("(defun g () (sort xs #'(lambda (a b) (< a b))))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(mapcar #'(lambda (x) x) xs)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_sharp_quoted_lambda_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build sharp-quoted lambda report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("lambda_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(mapcar (lambda (x) x) xs)").dialect_modelled);
    }

    /// The old report published only the path and span, so the envelope's own
    /// columns are the whole finding.
    #[test]
    fn a_finding_carries_its_line_and_nothing_the_envelope_does_not_already_print() {
        let report = report("(defun g ()\n  (sort xs #'(lambda (a b) (< a b))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "sharp-quoted-lambda");
        assert!(finding.text_columns().is_empty());
        assert!(finding.json_fields().is_empty());
    }

    #[test]
    fn the_summary_counts_every_lambda_scanned_not_only_the_flagged_ones() {
        let report = report("#'(lambda (x) x)\n(lambda (y) y)\n#'(lambda (z) z)\n");
        assert_eq!(report.summary, vec![("lambda_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
