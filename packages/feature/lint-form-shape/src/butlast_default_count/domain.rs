//! Common Lisp redundant-`butlast`-count detection: a call `(butlast list 1)`
//! (or the destructive `(nbutlast list 1)`) whose trailing count argument is the
//! literal `1`. CLHS defines both as `butlast list &optional (n 1)` — the count
//! defaults to `1`, so `(butlast list 1)` restates the default and is exactly
//! `(butlast list)`.
//!
//! The destructive `nbutlast` is included because dropping the redundant explicit
//! `1` preserves its mutation behavior exactly — `(nbutlast list 1)` and
//! `(nbutlast list)` perform the identical splice.
//!
//! Only the exact three-element shape `(butlast x 1)` is flagged, with the count
//! a bare integer `1` literal (no reader prefixes); a non-`1` count, the
//! already-minimal `(butlast x)`, and a reader-conditional count are left alone.
//!
//! The fix deletes the redundant trailing ` 1` argument, leaving the rest
//! byte-identical, so the rule is auto-fixable.
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
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// The `butlast` family whose optional count defaults to `1` per CLHS.
const BUTLAST_HEADS: [&str; 2] = ["butlast", "nbutlast"];

/// Whether `view` is the bare integer `1` literal (no reader prefixes).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view).is_some_and(|t| t == "1")
}

#[derive(Debug, Clone)]
pub struct ButlastDefaultCountItem {
    /// The span of the whole `(butlast …)` call form.
    pub span: ByteSpan,
    /// The 1-based line the call form starts on.
    pub line: usize,
    /// The span to delete: the trailing ` 1` count argument.
    ///
    /// The rewrite's input, and also published: the old report printed it, and
    /// a caller applying the deletion itself has no other way to locate it.
    pub removal_span: ByteSpan,
}

impl Finding for ButlastDefaultCountItem {
    /// The rule's own name. Every finding here is the same defect — a count
    /// argument that restates the default — with nothing to sort it by.
    fn kind(&self) -> &'static str {
        "butlast-default-count"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    /// Nothing beyond the path and line the envelope already prints: the old
    /// text row carried exactly those.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "removal_span",
            json!({
                "start": self.removal_span.start().get(),
                "end": self.removal_span.end().get(),
            }),
        )]
    }

    /// The same sentence the `butlast-default-count` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one defect described one way.
    fn message(&self) -> String {
        "explicit count of 1 restates butlast's default; (butlast x 1) is (butlast x)".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    call_form_count: &mut usize,
    violations: &mut Vec<ButlastDefaultCountItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !BUTLAST_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    // children: [butlast, list, 1] — exactly the list plus an explicit count.
    if view.children.len() != 3 {
        return;
    }
    if !is_one_literal(&view.children[2]) {
        return;
    }
    let removal_span = ByteSpan::new(view.children[1].span.end(), view.children[2].span.end());
    violations.push(ButlastDefaultCountItem {
        span: view.span,
        line: line_of(source, view.span.start().get()),
        removal_span,
    });
}

/// Collects every `(butlast list 1)` / `(nbutlast list 1)` in one file, with the
/// number of `butlast`/`nbutlast` calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant count here" for Common Lisp
/// and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_butlast_default_count_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<ButlastDefaultCountItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("call_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut call_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, source, &mut call_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("call_form_count", json!(call_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<ButlastDefaultCountItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_butlast_default_count_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build butlast default count report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<ButlastDefaultCountItem>) {
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
        let source = "(butlast xs 1)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " 1");
    }

    #[test]
    fn flags_nbutlast() {
        let (_, violations) = calls("(nbutlast xs 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_bare_butlast() {
        let (count, violations) = calls("(butlast xs)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_count() {
        let (_, violations) = calls("(butlast xs 3)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(BUTLAST xs 1)");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(butlast xs 1)", Dialect::Clojure).expect("parse");
        let report =
            build_butlast_default_count_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build butlast default count report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(butlast xs)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun f (xs)\n  (butlast xs 1))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "butlast-default-count");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![(
                "removal_span",
                json!({
                    "start": finding.removal_span.start().get(),
                    "end": finding.removal_span.end().get(),
                })
            )]
        );
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report("(butlast xs 1)\n(butlast ys)\n(nbutlast zs 3)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
