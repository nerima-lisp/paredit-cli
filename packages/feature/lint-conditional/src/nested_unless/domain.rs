//! Common Lisp nested-`unless` detection: an `unless` whose *only* body form is
//! itself an `unless`. `unless` runs its body when the test is nil, so
//! `(unless a (unless b body…))` runs `body` exactly when both `a` and `b` are
//! nil — which is precisely when `(or a b)` is nil. Thus the nesting is exactly
//! `(unless (or a b) body…)`: same guard, same body, same result, one fewer
//! level of indentation.
//!
//! Only the tightly-nested shape is flagged: the outer `unless` must have
//! exactly one body form, and that form must be an `unless` with at least a
//! test. An outer `unless` with additional body forms after the inner `unless`
//! (`(unless a (unless b c) d)`) is left alone — `d` runs whenever `a` is nil,
//! regardless of `b`, so the merge would change its guard. A reader-conditional
//! test is left alone as well (build-dependent).
//!
//! The fix rewrites `(unless a (unless b body…))` as `(unless (or a b) body…)`,
//! copying both tests and the inner body from their exact source, so the rule is
//! auto-fixable. When the outer test is already an `or`, the resulting
//! `(or (or …) b)` is flattened by the `nested-boolean` rule on a later pass.
//!
//! This is the `or`-combining mirror of
//! [`crate::nested_when::domain`], which combines nested `when` tests
//! with `and`.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// Whether `view` is an `(unless …)` form.
fn is_unless(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("unless"))
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// test containing one has no settled value.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct NestedUnlessItem {
    /// The span of the whole outer `(unless a (unless b …))` form.
    pub span: ByteSpan,
    /// The 1-based line the outer `unless` starts on.
    pub line: usize,
    /// The span of the outer test `a`.
    ///
    /// The rewrite's input, not the report's: the lint rule slices it to build
    /// the merged `(or a b)`, and neither the old renderer nor this one prints
    /// it.
    pub outer_test_span: ByteSpan,
    /// The span of the inner test `b`. Unreported, for the same reason as
    /// `outer_test_span`.
    pub inner_test_span: ByteSpan,
    /// The span covering the inner `unless`'s body forms (`None` when it has
    /// none). Unreported, for the same reason as `outer_test_span`.
    pub inner_body_span: Option<ByteSpan>,
}

impl Finding for NestedUnlessItem {
    /// The rule's own name. There is no sub-classification to make here — every
    /// finding is the same shape, an `unless` wrapping an `unless`.
    fn kind(&self) -> &'static str {
        "nested-unless"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
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

    /// The same sentence the `nested-unless` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "unless whose only body is an unless merges by or; (unless a (unless b c)) is (unless (or a b) c)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_unless(
    view: &ExpressionView,
    source: &str,
    unless_form_count: &mut usize,
    violations: &mut Vec<NestedUnlessItem>,
) {
    if !is_unless(view) {
        return;
    }
    *unless_form_count += 1;

    // Outer must be exactly [unless, test, single-body-form].
    if view.children.len() != 3 {
        return;
    }
    let outer_test = &view.children[1];
    let inner = &view.children[2];
    if is_reader_conditional(outer_test) {
        return;
    }
    // The single body form must itself be an `unless` with at least a test.
    if !is_unless(inner) || inner.children.len() < 2 {
        return;
    }
    let inner_test = &inner.children[1];
    if is_reader_conditional(inner_test) {
        return;
    }

    // Inner body spans from the first body form through the last; `None` when
    // the inner `unless` is just a test.
    let inner_body_span = if inner.children.len() > 2 {
        Some(ByteSpan::new(
            inner.children[2].span.start(),
            inner.children[inner.children.len() - 1].span.end(),
        ))
    } else {
        None
    };

    violations.push(NestedUnlessItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        outer_test_span: outer_test.span,
        inner_test_span: inner_test.span,
        inner_body_span,
    });
}

/// Collects every `unless` whose sole body form is an `unless` in one file, with
/// the number of `unless` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nested unless here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_nested_unless_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NestedUnlessItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("unless_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut unless_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_unless(subview, source, &mut unless_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("unless_form_count", json!(unless_form_count))],
    ))
}

fn line_of(source: &str, offset: usize) -> usize {
    1 + source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NestedUnlessItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nested_unless_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nested unless report")
    }

    /// The `(unless_form_count, violations)` pair the report is built from.
    fn nested(input: &str) -> (u64, Vec<NestedUnlessItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "unless_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("unless_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_unless_in_unless() {
        let source = "(unless a (unless b (do-it)))";
        let (count, violations) = nested(source);
        // Two unless forms scanned (outer and inner).
        assert_eq!(count, 2);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].outer_test_span), "a");
        assert_eq!(slice(source, violations[0].inner_test_span), "b");
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "(do-it)");
    }

    #[test]
    fn captures_multi_form_inner_body() {
        let source = "(unless a (unless b c d e))";
        let (_, violations) = nested(source);
        let body = violations[0].inner_body_span.expect("inner body span");
        assert_eq!(slice(source, body), "c d e");
    }

    #[test]
    fn preserves_compound_tests() {
        let source = "(unless (done-p x) (unless (< n 0) (go)))";
        let (_, violations) = nested(source);
        assert_eq!(slice(source, violations[0].outer_test_span), "(done-p x)");
        assert_eq!(slice(source, violations[0].inner_test_span), "(< n 0)");
    }

    #[test]
    fn does_not_flag_extra_outer_body_form() {
        // d runs whenever a is nil, regardless of b; not mergeable.
        let (_, violations) = nested("(unless a (unless b c) d)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_unless_body() {
        let (_, violations) = nested("(unless a (when b c))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_outer_unless() {
        let (_, violations) = nested("(unless a)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_both_unlesses() {
        let (_, violations) = nested("(UNLESS a (Unless b c))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_unless_deeper_in_a_form() {
        let (_, violations) = nested("(defun f (a b) (unless a (unless b (g))))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(unless a (unless b c))", Dialect::Clojure)
            .expect("parse");
        let report = build_nested_unless_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nested unless report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("unless_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(unless a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_no_columns_of_its_own() {
        let report = report("(defun f (a b)\n  (unless a (unless b (g))))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "nested-unless");
        assert!(finding.json_fields().is_empty());
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_unless_scanned_not_only_the_flagged_ones() {
        let report = report("(unless a (unless b c))\n(unless d e)\n");
        // Three unless forms (the nested pair and the standalone one), one
        // finding.
        assert_eq!(report.summary, vec![("unless_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
