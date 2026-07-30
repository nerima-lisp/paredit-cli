//! Common Lisp redundant-`:count`-`nil` detection: a call to a standard
//! bounded-sequence operator with an explicit `:count nil`. For these operators
//! CLHS specifies that `:count` *defaults to* `nil` (meaning "unlimited"), so
//! `(remove x seq :count nil)` is exactly `(remove x seq)` — the explicit count
//! restates the default.
//!
//! Scope is gated to the removing/substituting operators whose `:count` defaults
//! to `nil` (`COUNT_HEADS` — `remove`, `delete`, `substitute`, `nsubstitute`,
//! and their `-if`/`-if-not` variants). Operators that do not take a `:count`
//! keyword (`find`, `position`, …) are not touched. Only a bare `nil` literal
//! value is flagged.
//!
//! The fix deletes the redundant ` :count nil` argument pair (from the end of the
//! preceding argument through the `nil`), leaving the rest byte-identical, so the
//! rule is auto-fixable.
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

/// Removing/substituting operators whose `:count` keyword defaults to `nil` per CLHS.
const COUNT_HEADS: [&str; 12] = [
    "remove",
    "remove-if",
    "remove-if-not",
    "delete",
    "delete-if",
    "delete-if-not",
    "substitute",
    "substitute-if",
    "substitute-if-not",
    "nsubstitute",
    "nsubstitute-if",
    "nsubstitute-if-not",
];

/// Whether `view` is the bare `:count` keyword atom.
fn is_count_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":count"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantCountNilItem {
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The span to delete: the ` :count nil` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, as spelled at the call site.
    pub head: String,
}

impl Finding for RedundantCountNilItem {
    /// The rule's own name. The operator varies per finding, but it is a
    /// source-cased `String` off the call site rather than a canonical tag, so
    /// it stays data in `head` and the kind names the rule.
    fn kind(&self) -> &'static str {
        "redundant-count-nil"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.head.clone()]
    }

    /// `removal_span` is a fix input, but this report has always published it,
    /// so it stays: a consumer scripting the deletion around this command
    /// depends on it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("head", json!(self.head)),
            (
                "removal_span",
                json!({
                    "start": self.removal_span.start().get(),
                    "end": self.removal_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `redundant-count-nil` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} :count defaults to nil; drop the explicit :count nil",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantCountNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !COUNT_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_count_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantCountNilItem {
            span: view.span,
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every bounded-sequence call with a redundant `:count nil` in one
/// file, with the number of such calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `:count nil` here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_redundant_count_nil_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantCountNilItem>> {
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

    fn report(input: &str) -> FileFindings<RedundantCountNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_count_nil_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant count nil report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<RedundantCountNilItem>) {
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
    fn flags_count_nil() {
        let source = "(remove x seq :count nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :count nil");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :count nil :from-end t)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :count nil");
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(remove x seq :count 3)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // find does not take a :count keyword.
        let (count, violations) = calls("(find x seq :count nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(REMOVE x seq :count nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (delete y xs :count nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(remove x seq :count nil)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_redundant_count_nil_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build redundant count nil report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(remove x seq)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_removal_span() {
        let source = "(defun f (x seq)\n  (remove x seq :count nil))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "redundant-count-nil");
        assert_eq!(finding.text_columns(), vec!["remove".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("remove")),
                (
                    "removal_span",
                    json!({
                        "start": finding.removal_span.start().get(),
                        "end": finding.removal_span.end().get(),
                    })
                ),
            ]
        );
        assert_eq!(slice(source, finding.removal_span), " :count nil");
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(remove x seq :count nil)\n(delete y xs)\n(remove z zs :count 3)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
