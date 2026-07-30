//! Common Lisp `list*`-with-`nil`-tail detection: a `(list* a … nil)` whose
//! final argument is the literal `nil`. `list*` conses every argument but the
//! last onto that last argument as the tail; with a `nil` tail the result is a
//! proper list, exactly what `list` builds — `(list* a b nil)` is
//! `(cons a (cons b nil))` is `(list a b)`. The `list*`-against-`nil` spelling is
//! a spelled-out `list`.
//!
//! Only a bare `nil` final argument is matched, and only with at least one
//! preceding element (`(list* a nil)` → `(list a)`). A single-argument
//! `(list* nil)` (which is just `nil`) is
//! [`crate::single_operand_list_op::domain`]'s concern, a non-`nil` final
//! argument is a genuine `list*`, and a reader-conditional operand is left alone.
//!
//! The fix rewrites the head `list*` to `list` and deletes the trailing ` nil`,
//! leaving the kept elements byte-identical, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct ListStarNilItem {
    /// The span of the whole `(list* …)` call form.
    pub span: ByteSpan,
    /// The span of the `list*` head token, rewritten to `list`.
    ///
    /// The rewrite's input, but the old report published it and a consumer
    /// applying its own two-part edit needs it, so it stays on the report.
    pub head_span: ByteSpan,
    /// The span to delete: the trailing ` nil` tail argument. Published for the
    /// same reason as `head_span`.
    pub removal_span: ByteSpan,
}

impl Finding for ListStarNilItem {
    /// The rule's own name. Every finding is the same `nil`-tailed `list*`, so
    /// there is nothing for a per-finding discriminator to say.
    fn kind(&self) -> &'static str {
        "list-star-nil"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried exactly those two. `message` carries the description.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head_span", span_json(self.head_span)),
            ("removal_span", span_json(self.removal_span)),
        ]
    }

    /// The same sentence the `list-star-nil` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "list* with a nil tail is a spelled-out list; (list* a b nil) is (list a b)".to_owned()
    }
}

/// A sub-span in the shape the old hand-written report published it.
fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    call_form_count: &mut usize,
    violations: &mut Vec<ListStarNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("list*") {
        return;
    }
    *call_form_count += 1;

    // children: [list*, elem, …, nil] — at least one element before the tail.
    if view.children.len() < 3 {
        return;
    }
    let last = &view.children[view.children.len() - 1];
    if !is_nil_literal(last) {
        return;
    }
    let prev = &view.children[view.children.len() - 2];
    let removal_span = ByteSpan::new(prev.span.end(), last.span.end());
    violations.push(ListStarNilItem {
        span: view.span,
        head_span: view.children[0].span,
        removal_span,
    });
}

/// Collects every `(list* a … nil)` in one file, with the number of `list*`
/// calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no nil-tailed list* here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_list_star_nil_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ListStarNilItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ListStarNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_list_star_nil_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build list* nil report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<ListStarNilItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "call_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("call_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_elements_and_nil() {
        let source = "(list* a b nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "list*");
        assert_eq!(slice(source, violations[0].removal_span), " nil");
    }

    #[test]
    fn flags_one_element_and_nil() {
        let (_, violations) = calls("(list* a nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_non_nil_tail() {
        // (list* a b) is a genuine two-argument list* — cons-to-list's dual.
        assert!(calls("(list* a b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_single_nil() {
        // (list* nil) is just nil — single-operand-list-op's concern.
        assert!(calls("(list* nil)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(LIST* a b nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(apply #'f (list* a b nil))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(list* a b nil)", Dialect::Clojure).expect("parse");
        let report = build_list_star_nil_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build list* nil report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(list* a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_spans_the_fix_needs() {
        let source = "(defun f (a b)\n  (list* a b nil))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "list-star-nil");
        assert_eq!(slice(source, finding.head_span), "list*");
        assert_eq!(slice(source, finding.removal_span), " nil");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head_span", span_json(finding.head_span)),
                ("removal_span", span_json(finding.removal_span)),
            ]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_list_star_call_not_only_the_flagged_ones() {
        let report = report("(list* a b nil)\n(list* a b)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
