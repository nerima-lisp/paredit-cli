//! Common Lisp redundant-`last`-count detection: a call `(last list 1)` whose
//! trailing count argument is the literal `1`. CLHS defines `last` as
//! `last list &optional (n 1)` — the count defaults to `1`, so `(last list 1)`
//! restates the default and is exactly `(last list)`.
//!
//! Only the exact three-element shape `(last x 1)` is flagged, with the count a
//! bare integer `1` literal (no reader prefixes); a non-`1` count (`(last x 2)`),
//! the already-minimal `(last x)`, and a reader-conditional count are left alone.
//!
//! The fix deletes the redundant trailing ` 1` argument (from the end of the
//! list argument through the `1`), leaving the rest byte-identical, so the rule
//! is auto-fixable.
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

/// Whether `view` is the bare integer `1` literal (no reader prefixes).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|t| t == "1")
}

#[derive(Debug, Clone)]
pub struct LastDefaultCountItem {
    /// The span of the whole `(last …)` call form.
    pub span: ByteSpan,
    /// The span to delete: the trailing ` 1` count argument.
    ///
    /// The rewrite's input, but the old report published it and a consumer
    /// applying its own deletion needs it, so it stays on the report.
    pub removal_span: ByteSpan,
}

impl Finding for LastDefaultCountItem {
    /// The rule's own name. Every finding is the same redundant `1`, so there
    /// is nothing for a per-finding discriminator to say.
    fn kind(&self) -> &'static str {
        "last-default-count"
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
        vec![("removal_span", span_json(self.removal_span))]
    }

    /// The same sentence the `last-default-count` lint rule writes, so a SARIF
    /// or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "explicit count of 1 restates last's default; (last x 1) is (last x)".to_owned()
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
    violations: &mut Vec<LastDefaultCountItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("last") {
        return;
    }
    *call_form_count += 1;

    // children: [last, list, 1] — exactly the list plus an explicit count.
    if view.children.len() != 3 {
        return;
    }
    if !is_one_literal(&view.children[2]) {
        return;
    }
    let removal_span = ByteSpan::new(view.children[1].span.end(), view.children[2].span.end());
    violations.push(LastDefaultCountItem {
        span: view.span,
        removal_span,
    });
}

/// Collects every `(last list 1)` in one file, with the number of `last` calls
/// scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant count here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_last_default_count_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<LastDefaultCountItem>> {
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

    fn report(input: &str) -> FileFindings<LastDefaultCountItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_last_default_count_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build last default count report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<LastDefaultCountItem>) {
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
    fn flags_explicit_one() {
        let source = "(last xs 1)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " 1");
    }

    #[test]
    fn does_not_flag_bare_last() {
        let (count, violations) = calls("(last xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_count() {
        let (_, violations) = calls("(last xs 2)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(LAST xs 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(car (last xs 1))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(last xs 1)", Dialect::Clojure).expect("parse");
        let report = build_last_default_count_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build last default count report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(last xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_the_removal_span_the_fix_needs() {
        let source = "(defun f (xs)\n  (last xs 1))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "last-default-count");
        assert_eq!(slice(source, finding.removal_span), " 1");
        assert_eq!(
            finding.json_fields(),
            vec![("removal_span", span_json(finding.removal_span))]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_last_call_not_only_the_flagged_ones() {
        let report = report("(last xs 1)\n(last ys)\n(last zs 2)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
