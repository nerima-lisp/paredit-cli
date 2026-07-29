//! Common Lisp negated-`when`/`unless` detection: a `(when (not X) …)` or
//! `(unless (not X) …)` whose test is a single negation. `when` and `unless`
//! are exact complements, so negating the test just to pick one is a
//! double-negative that reads backwards — `(when (not ready) …)` is more
//! directly written `(unless ready …)`, and `(unless (not ready) …)` as
//! `(when ready …)`.
//!
//! Both `not` and its list-specific synonym `null` count as the negation (they
//! are interchangeable on a generalized boolean). Only a negation of *exactly
//! one* argument is flagged — a malformed `(not)` or `(not a b)` is left for the
//! arity lints, and its rewrite is not well defined.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding, line_of};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};
use serde_json::{Value, json};

/// The complement macro for a `when`/`unless` head: flipping the conditional is
/// half of removing the redundant negation.
fn complement_head(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("when") {
        Some("unless")
    } else if head.eq_ignore_ascii_case("unless") {
        Some("when")
    } else {
        None
    }
}

/// The negation operator of a `(not X)` / `(null X)` test with exactly one
/// argument, lowercased, or `None` if the view is not such a negation.
fn single_negation(view: &ExpressionView) -> Option<&'static str> {
    if !is_paren_list(view) || view.children.len() != 2 {
        return None;
    }
    let head = list_head(view)?;
    if head.eq_ignore_ascii_case("not") {
        Some("not")
    } else if head.eq_ignore_ascii_case("null") {
        Some("null")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct NegatedWhenUnlessItem {
    /// The span of the whole `(when …)`/`(unless …)` form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span of the head symbol (`when`/`unless`), for a flip-the-macro fix.
    pub head_span: ByteSpan,
    /// The span of the whole `(not X)`/`(null X)` test, replaced by `X`.
    pub test_span: ByteSpan,
    /// The span of the negation's sole argument `X` (the un-negated test).
    pub inner_span: ByteSpan,
    /// The conditional macro as written, lowercased (`when` or `unless`).
    pub head: &'static str,
    /// The negation operator (`not` or `null`).
    pub negator: &'static str,
    /// The macro the rewrite would use instead (the complement of `head`).
    pub suggested_head: &'static str,
}

impl Finding for NegatedWhenUnlessItem {
    /// The conditional macro, so `when` and `unless` are separable without
    /// parsing JSON.
    ///
    /// A closed, canonical pair — `complement_head` accepts nothing else and
    /// stores the lowercased form regardless of how the source spelled it — so
    /// it is a fixed vocabulary rather than an echo of the file. They are also
    /// two different double-negatives, and a consumer filtering on one of them
    /// is asking a real question.
    fn kind(&self) -> &'static str {
        self.head
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// The negator and the suggestion, in the order the old text row had them.
    /// The head that led that row is now the leading `kind`.
    fn text_columns(&self) -> Vec<String> {
        vec![self.negator.to_owned(), self.suggested_head.to_owned()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            ("negator", json!(self.negator)),
            ("suggested_head", json!(self.suggested_head)),
        ]
    }

    /// The same sentence the `negated-when-unless` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} test is ({} …); use {} on the un-negated test",
            self.head, self.negator, self.suggested_head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_conditional(
    view: &ExpressionView,
    source: &str,
    conditional_form_count: &mut usize,
    violations: &mut Vec<NegatedWhenUnlessItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(suggested_head) = complement_head(head) else {
        return;
    };
    *conditional_form_count += 1;

    // children[0] is the head; children[1] is the test (if present).
    let Some(test) = view.children.get(1) else {
        return;
    };
    let Some(negator) = single_negation(test) else {
        return;
    };
    // single_negation guarantees the test is `(not X)`/`(null X)` with exactly
    // two children, so children[1] (the un-negated argument) exists.
    let inner_span = test.children[1].span;
    // `head` is a canonical when/unless (verified by complement_head), so the
    // stored form is the complement of the suggestion.
    let canonical_head = complement_head(suggested_head).expect("complement is involutive");
    violations.push(NegatedWhenUnlessItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        head_span: view.children[0].span,
        test_span: test.span,
        inner_span,
        head: canonical_head,
        negator,
        suggested_head,
    });
}

/// Collects every negated `when`/`unless` in one file, with the number of
/// `when`/`unless` forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no negated test here" for Common Lisp
/// and "nothing was looked for" for Fennel, and the two read identically
/// without the flag.
pub fn build_negated_when_unless_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NegatedWhenUnlessItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("conditional_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut conditional_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_conditional(
                subview,
                source,
                &mut conditional_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("conditional_form_count", json!(conditional_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<NegatedWhenUnlessItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_negated_when_unless_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build negated when/unless report")
    }

    /// The `(conditional_form_count, violations)` pair the report is built from.
    fn conditionals(input: &str) -> (u64, Vec<NegatedWhenUnlessItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "conditional_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("conditional_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_when_with_a_not_test() {
        let (count, violations) = conditionals("(when (not ready) (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].negator, "not");
        assert_eq!(violations[0].suggested_head, "unless");
    }

    #[test]
    fn fix_spans_isolate_the_head_test_and_inner() {
        let input = "(when (not ready) (go))";
        let (_, violations) = conditionals(input);
        let item = &violations[0];
        assert_eq!(
            &input[item.head_span.start().get()..item.head_span.end().get()],
            "when"
        );
        assert_eq!(
            &input[item.test_span.start().get()..item.test_span.end().get()],
            "(not ready)"
        );
        assert_eq!(
            &input[item.inner_span.start().get()..item.inner_span.end().get()],
            "ready"
        );
    }

    #[test]
    fn flags_unless_with_a_not_test() {
        let (_, violations) = conditionals("(unless (not ready) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "unless");
        assert_eq!(violations[0].suggested_head, "when");
    }

    #[test]
    fn flags_a_null_test() {
        let (_, violations) = conditionals("(when (null lst) (init))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].negator, "null");
        assert_eq!(violations[0].suggested_head, "unless");
    }

    #[test]
    fn case_folds_heads_and_negators() {
        let (_, violations) = conditionals("(WHEN (NOT x) y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
        assert_eq!(violations[0].negator, "not");
    }

    #[test]
    fn does_not_flag_a_plain_test() {
        let (count, violations) = conditionals("(when ready (go))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_nested_call_that_is_not_a_negation() {
        let (_, violations) = conditionals("(when (evenp n) (go))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_multi_argument_not() {
        // (not a b) is malformed arity, not a single negation; leave it be.
        let (_, violations) = conditionals("(when (not a b) c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_zero_argument_not() {
        let (_, violations) = conditionals("(when (not) c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_if_or_cond() {
        let (count, violations) = conditionals("(if (not x) a b)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_nested_negation_test() {
        // (when (not (null x))) is still a single negation of one argument.
        let (_, violations) = conditionals("(when (not (null x)) y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].suggested_head, "unless");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(when (not x) y)", Dialect::Clojure).expect("parse");
        let report =
            build_negated_when_unless_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build negated when/unless report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("conditional_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(when ready (go))").dialect_modelled);
    }

    /// The head leads the row as the `kind`, and stays in the JSON where the
    /// old renderer published it.
    #[test]
    fn a_finding_carries_its_line_its_head_and_its_suggestion() {
        let report = report("(defun f ()\n  (when (not ready) (go)))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "when");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("when")),
                ("negator", json!("not")),
                ("suggested_head", json!("unless")),
            ]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["not".to_owned(), "unless".to_owned()]
        );
    }

    /// `unless` is the other half of the closed pair, so the two are separable
    /// on `kind` alone.
    #[test]
    fn an_unless_finding_is_a_different_kind_from_a_when_finding() {
        assert_eq!(
            report("(unless (not ok) (run))").findings[0].kind(),
            "unless"
        );
    }

    #[test]
    fn the_summary_counts_every_conditional_scanned_not_only_the_flagged_ones() {
        let report = report("(when (not x) y)\n(when ready (go))\n");
        assert_eq!(report.summary, vec![("conditional_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
