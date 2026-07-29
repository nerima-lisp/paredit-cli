//! Common Lisp redundant-`:from-end`-`nil` detection: a call to a standard
//! sequence operator with an explicit `:from-end nil`. For these operators
//! CLHS specifies that `:from-end` *defaults to* `nil`, so
//! `(find x seq :from-end nil)` is exactly `(find x seq)` — the explicit
//! `:from-end nil` restates the default.
//!
//! Scope is gated to the operators whose `:from-end` defaults to `nil`
//! (`FROM_END_HEADS` — `find`, `position`, `count`, `remove`, `substitute`,
//! the `-if`/`-if-not` variants, `remove-duplicates`, `reduce`, `search`,
//! `mismatch`, …). Only a bare `nil` literal value is flagged.
//!
//! The fix deletes the redundant ` :from-end nil` argument pair (from the end of
//! the preceding argument through the `nil`), leaving the rest byte-identical, so
//! the rule is auto-fixable.
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

/// Sequence operators whose `:from-end` keyword defaults to `nil` per CLHS.
const FROM_END_HEADS: [&str; 26] = [
    "find",
    "find-if",
    "find-if-not",
    "position",
    "position-if",
    "position-if-not",
    "count",
    "count-if",
    "count-if-not",
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
    "remove-duplicates",
    "delete-duplicates",
    "reduce",
    "search",
    "mismatch",
];

/// Whether `view` is the bare `:from-end` keyword atom.
fn is_from_end_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":from-end"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct RedundantFromEndNilItem {
    /// The span of the whole call form.
    pub span: ByteSpan,
    /// The 1-based line the call starts on.
    pub line: usize,
    /// The span to delete: the ` :from-end nil` argument pair.
    pub removal_span: ByteSpan,
    /// The operator name, as spelled at the call site.
    pub head: String,
}

impl Finding for RedundantFromEndNilItem {
    /// The rule's own name. The operator varies per finding, but it is a
    /// source-cased `String` off the call site rather than a canonical tag, so
    /// it stays data in `head` and the kind names the rule.
    fn kind(&self) -> &'static str {
        "redundant-from-end-nil"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
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

    /// The same sentence the `redundant-from-end-nil` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} :from-end defaults to nil; drop the explicit :from-end nil",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    call_form_count: &mut usize,
    violations: &mut Vec<RedundantFromEndNilItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !FROM_END_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_from_end_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(RedundantFromEndNilItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
            head: head.to_owned(),
        });
        return;
    }
}

/// Collects every sequence call with a redundant `:from-end nil` in one file,
/// with the number of such calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no redundant `:from-end nil` here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_redundant_from_end_nil_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<RedundantFromEndNilItem>> {
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

    fn report(input: &str) -> FileFindings<RedundantFromEndNilItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_redundant_from_end_nil_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build redundant from-end nil report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<RedundantFromEndNilItem>) {
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
    fn flags_from_end_nil() {
        let source = "(find x seq :from-end nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].removal_span), " :from-end nil");
    }

    #[test]
    fn removal_keeps_other_keywords() {
        let source = "(remove x seq :from-end nil :count 3)";
        let (_, violations) = calls(source);
        assert_eq!(slice(source, violations[0].removal_span), " :from-end nil");
    }

    #[test]
    fn does_not_flag_non_nil() {
        let (count, violations) = calls("(find x seq :from-end t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_allowlisted_head() {
        // sort is not in the :from-end-defaulting allowlist.
        let (count, violations) = calls("(sort xs #'< :from-end nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(FIND x seq :from-end nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(when (position y xs :from-end nil) (go))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(find x seq :from-end nil)", Dialect::Clojure)
            .expect("parse");
        let report =
            build_redundant_from_end_nil_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build redundant from-end nil report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(find x seq)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_its_head_and_its_removal_span() {
        let source = "(defun f (x seq)\n  (find x seq :from-end nil))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "redundant-from-end-nil");
        assert_eq!(finding.text_columns(), vec!["find".to_owned()]);
        assert_eq!(
            finding.json_fields(),
            vec![
                ("head", json!("find")),
                (
                    "removal_span",
                    json!({
                        "start": finding.removal_span.start().get(),
                        "end": finding.removal_span.end().get(),
                    })
                ),
            ]
        );
        assert_eq!(slice(source, finding.removal_span), " :from-end nil");
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report =
            report("(find x seq :from-end nil)\n(position y xs)\n(count z zs :from-end t)\n");
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
