//! Common Lisp nested-`when` detection: a `when` whose *only* body form is
//! itself a `when`. `(when a (when b body…))` runs `body` exactly when both `a`
//! and `b` are true, which is precisely `(when (and a b) body…)` — same guard,
//! same body, same result. Collapsing the two tests into one `and` removes a
//! level of indentation and states the combined condition directly.
//!
//! Only the tightly-nested shape is flagged: the outer `when` must have exactly
//! one body form, and that form must be a `when` with at least a test. An outer
//! `when` with additional body forms after the inner `when`
//! (`(when a (when b c) d)`) is left alone — `d` runs whenever `a` holds,
//! regardless of `b`, so the merge would change its guard. A reader-conditional
//! test is left alone as well (build-dependent).
//!
//! The fix rewrites `(when a (when b body…))` as `(when (and a b) body…)`,
//! copying both tests and the inner body from their exact source, so the rule is
//! auto-fixable. When the outer test is already an `and`, the resulting
//! `(and (and …) b)` is flattened by the `nested-boolean` rule on a later pass.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether `view` is a `(when …)` form.
fn is_when(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("when"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// test containing one has no settled value.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedWhenItem {
    /// The span of the whole outer `(when a (when b …))` form.
    pub span: ByteSpan,
    /// The span of the outer test `a`.
    ///
    pub outer_test_span: ByteSpan,
    /// The span of the inner test `b`. Unreported, for the same reason as
    /// `outer_test_span`.
    pub inner_test_span: ByteSpan,
    /// The span covering the inner `when`'s body forms (`None` when it has
    /// none). Unreported, for the same reason as `outer_test_span`.
    pub inner_body_span: Option<ByteSpan>,
}

impl Finding for NestedWhenItem {
    /// The rule's own name. There is no sub-classification to make here — every
    /// finding is the same shape, a `when` wrapping a `when`.
    fn kind(&self) -> &'static str {
        "nested-when"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None. The old renderer printed the path and the offset and nothing else,
    /// and both are the envelope's now.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// None, for the same reason: the old JSON carried only `path` and `span`.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn message(&self) -> String {
        "when whose only body is a when merges by and; (when a (when b c)) is (when (and a b) c)"
            .to_owned()
    }
}

pub fn examine_when(
    view: &ExpressionView,
    when_form_count: &mut usize,
    violations: &mut Vec<NestedWhenItem>,
) {
    if !is_when(view) {
        return;
    }
    *when_form_count += 1;

    // Outer must be exactly [when, test, single-body-form].
    if view.children.len() != 3 {
        return;
    }
    let outer_test = &view.children[1];
    let inner = &view.children[2];
    if is_reader_conditional(outer_test) {
        return;
    }
    // The single body form must itself be a `when` with at least a test.
    if !is_when(inner) || inner.children.len() < 2 {
        return;
    }
    let inner_test = &inner.children[1];
    if is_reader_conditional(inner_test) {
        return;
    }

    // Inner body spans from the first body form through the last; `None` when
    // the inner `when` is just a test.
    let inner_body_span = if inner.children.len() > 2 {
        Some(ByteSpan::new(
            inner.children[2].span.start(),
            inner.children[inner.children.len() - 1].span.end(),
        ))
    } else {
        None
    };

    violations.push(NestedWhenItem {
        span: view.span,
        outer_test_span: outer_test.span,
        inner_test_span: inner_test.span,
        inner_body_span,
    });
}

/// Collects every `when` whose sole body form is a `when` in one file, with the
/// number of `when` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_nested_when_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedWhenItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("when_form_count", json!(0))],
        ));
    }

    let mut when_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_when(subview, &mut when_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("when_form_count", json!(when_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedWhenItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_when_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested when report")
    }

    fn nested(input: &str) -> (u64, Vec<NestedWhenItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "when_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("when_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_when_in_when() {
        let source = "(when a (when b (do-it)))";
        let (count, violations) = nested(source);
        // Two when forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_test_span), "a");
        assert_eq!(slice(source, violations[0].inner_test_span), "b");
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "(do-it)");
    }

    #[test]
    fn captures_multi_form_inner_body() {
        let source = "(when a (when b c d e))";
        let (_, violations) = nested(source);
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "c d e");
    }

    #[test]
    fn preserves_compound_tests() {
        let source = "(when (ready-p x) (when (> n 0) (go)))";
        let (_, violations) = nested(source);
        assert_eq!(slice(source, violations[0].outer_test_span), "(ready-p x)");
        assert_eq!(slice(source, violations[0].inner_test_span), "(> n 0)");
    }

    #[test]
    fn does_not_flag_extra_outer_body_form() {
        // d runs whenever a holds, regardless of b; not mergeable.
        let (_, violations) = nested("(when a (when b c) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_when_body() {
        let (_, violations) = nested("(when a (unless b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_outer_when() {
        // No body at all.
        let (_, violations) = nested("(when a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_both_whens() {
        let (_, violations) = nested("(WHEN a (When b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_when_deeper_in_a_form() {
        let (_, violations) = nested("(defun f (a b) (when a (when b (g))))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(when a (when b c))", Dialect::Clojure).expect("parse");
        let report = build_nested_when_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested when report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("when_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(when a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_columns_of_its_own() {
        let report = report("(defun f (a b)\n  (when a (when b (g))))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "nested-when");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_when_scanned_not_only_the_flagged_ones() {
        let report = report("(when a (when b c))\n(when d e)\n");
        // Three when forms (the nested pair and the standalone one), one
        // finding.
        assert_eq!(report.summary, vec![("when_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
