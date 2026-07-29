//! Common Lisp redundant-`make-list`-`:initial-element`-`nil` detection: a call
//! `(make-list n :initial-element nil)`. CLHS defines `make-list` as
//! `make-list size &key initial-element` with `initial-element` defaulting to
//! `nil`, so `(make-list n :initial-element nil)` restates the default and is
//! exactly `(make-list n)`.
//!
//! Only a bare `nil` literal value (no reader prefixes) is flagged; a non-`nil`
//! `:initial-element` (`(make-list n :initial-element 0)`) is meaningful and left
//! alone.
//!
//! The fix deletes the redundant ` :initial-element nil` argument pair (from the
//! end of the preceding argument through the `nil`), leaving the rest
//! byte-identical, so the rule is auto-fixable.
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

/// Whether `view` is the bare `:initial-element` keyword atom.
fn is_initial_element_keyword(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case(":initial-element"))
}

/// Whether `view` is the bare `nil` literal (no reader prefixes).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|t| t.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct MakeListDefaultElementItem {
    /// The span of the whole `(make-list …)` call form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The span to delete: the ` :initial-element nil` argument pair.
    ///
    /// Both the fix's input and part of the report: an agent that wants to
    /// perform the deletion itself needs the exact bytes, and the old report
    /// published them.
    pub removal_span: ByteSpan,
}

impl Finding for MakeListDefaultElementItem {
    /// The rule's own name. Every finding here is the same defect — an explicit
    /// `:initial-element nil` — with nothing to sub-divide it by.
    fn kind(&self) -> &'static str {
        "make-list-default-element"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

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

    /// The same sentence the `make-list-default-element` lint rule writes, so a
    /// SARIF or JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "explicit :initial-element nil restates make-list's default; drop it".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    source: &str,
    call_form_count: &mut usize,
    violations: &mut Vec<MakeListDefaultElementItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("make-list") {
        return;
    }
    *call_form_count += 1;

    for index in 1..view.children.len().saturating_sub(1) {
        if !is_initial_element_keyword(&view.children[index]) {
            continue;
        }
        if !is_nil_literal(&view.children[index + 1]) {
            continue;
        }
        let removal_span = ByteSpan::new(
            view.children[index - 1].span.end(),
            view.children[index + 1].span.end(),
        );
        violations.push(MakeListDefaultElementItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            removal_span,
        });
        return;
    }
}

/// Collects every `(make-list n :initial-element nil)` in one file, with the
/// number of `make-list` calls scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no restated default" for Common Lisp and
/// "nothing was looked for" for Clojure, and the two read identically without
/// the flag.
pub fn build_make_list_default_element_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MakeListDefaultElementItem>> {
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

    fn report(input: &str) -> FileFindings<MakeListDefaultElementItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_make_list_default_element_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build make-list default element report")
    }

    /// The `(call_form_count, violations)` pair the report is built from.
    fn calls(input: &str) -> (u64, Vec<MakeListDefaultElementItem>) {
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
    fn flags_initial_element_nil() {
        let source = "(make-list n :initial-element nil)";
        let (count, violations) = calls(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(
            slice(source, violations[0].removal_span),
            " :initial-element nil"
        );
    }

    #[test]
    fn does_not_flag_non_nil_element() {
        let (count, violations) = calls("(make-list n :initial-element 0)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_bare_make_list() {
        let (_, violations) = calls("(make-list n)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = calls("(MAKE-LIST n :INITIAL-ELEMENT nil)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested_call() {
        let (_, violations) = calls("(setf x (make-list 5 :initial-element nil))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(make-list n :initial-element nil)", Dialect::Clojure)
                .expect("parse");
        let report =
            build_make_list_default_element_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build make-list default element report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("call_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(make-list n)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_removal_span() {
        let report = report("(defun f (n)\n  (make-list n :initial-element nil))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "make-list-default-element");
        assert_eq!(
            finding.json_fields(),
            vec![(
                "removal_span",
                json!({
                    "start": finding.removal_span.start().get(),
                    "end": finding.removal_span.end().get(),
                }),
            )]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_call_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(make-list n)\n(make-list n :initial-element 0)\n(make-list n :initial-element nil)\n",
        );
        assert_eq!(report.summary, vec![("call_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
