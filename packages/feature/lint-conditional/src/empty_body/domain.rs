//! Common Lisp empty-body detection: a `when`, `unless`, `dolist`, or `dotimes`
//! form that has its test (or iteration spec) but no body — `(when ready)`,
//! `(unless done)`, `(dolist (x items))`, `(dotimes (i n))`. The test or spec is
//! evaluated and then nothing happens, so the form is pointless: `(when x)` is
//! just `(progn x nil)`, and an empty `dolist`/`dotimes` iterates doing nothing.
//! This is almost always a forgotten body or an editing leftover.
//!
//! Only these four forms are covered, and only when the fixed prefix is present
//! (the test for `when`/`unless`, the `(var …)` spec for `dolist`/`dotimes`) but
//! no body form follows. A form with a body, or a malformed form missing its
//! prefix entirely, is left alone.
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
use paredit_core_syntax::view_query::{for_each_subview, list_head};
use serde_json::{Value, json};

/// The index at which a body-requiring form's body begins, or `None` if the
/// head is not one of the covered forms. All four take a single prefix element
/// (a test or a spec), so their body starts at index 2.
fn body_start(head: &str) -> Option<usize> {
    if head.eq_ignore_ascii_case("when")
        || head.eq_ignore_ascii_case("unless")
        || head.eq_ignore_ascii_case("dolist")
        || head.eq_ignore_ascii_case("dotimes")
    {
        Some(2)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
pub struct EmptyBodyItem {
    /// The span of the whole body-less form.
    pub span: ByteSpan,
    /// The 1-based line the form starts on.
    pub line: usize,
    /// The form head (`when`, `unless`, `dolist`, or `dotimes`).
    pub head: String,
}

impl Finding for EmptyBodyItem {
    /// The rule's own name rather than the head. The head is case-folded from
    /// the source at scan time and typed `String`, not one of a closed set of
    /// `&'static str` names, so it stays a JSON field and a column instead.
    fn kind(&self) -> &'static str {
        "empty-body"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn line(&self) -> usize {
        self.line
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("head={}", self.head)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("head", json!(self.head))]
    }

    /// The same sentence the `empty-body` lint rule writes, so a SARIF or JUnit
    /// consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!(
            "{} has no body; the test/spec runs, then nothing",
            self.head
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_form(
    view: &ExpressionView,
    source: &str,
    body_form_count: &mut usize,
    violations: &mut Vec<EmptyBodyItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(start) = body_start(head) else {
        return;
    };
    *body_form_count += 1;

    // The prefix element (test/spec) is present, but nothing follows it: the
    // children are exactly [head, prefix]. A shorter form is missing its prefix
    // (malformed, a different concern); a longer one has a body.
    if view.children.len() == start {
        violations.push(EmptyBodyItem {
            span: view.span,
            line: line_of(source, view.span.start().get()),
            head: head.to_ascii_lowercase(),
        });
    }
}

/// Collects every empty-bodied `when`/`unless`/`dolist`/`dotimes` in one file,
/// with the number of such forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "every body-taking form here has a body"
/// for Common Lisp and "nothing was looked for" for Fennel, and the two read
/// identically without the flag.
pub fn build_empty_body_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<EmptyBodyItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            Vec::new(),
            vec![("body_form_count", json!(0))],
        ));
    }

    let source = tree.source();
    let mut body_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, source, &mut body_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        violations,
        vec![("body_form_count", json!(body_form_count))],
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

    fn report(input: &str) -> FileFindings<EmptyBodyItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_empty_body_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build empty body report")
    }

    /// The `(body_form_count, violations)` pair the report is built from.
    fn bodies(input: &str) -> (u64, Vec<EmptyBodyItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "body_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("body_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_when_with_no_body() {
        let (count, violations) = bodies("(when ready)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "when");
    }

    #[test]
    fn flags_unless_with_no_body() {
        let (_, violations) = bodies("(unless done)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "unless");
    }

    #[test]
    fn flags_dolist_and_dotimes_with_no_body() {
        let (_, violations) = bodies("(dolist (x items))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "dolist");
        let (_, violations) = bodies("(dotimes (i n))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "dotimes");
    }

    #[test]
    fn does_not_flag_a_form_with_a_body() {
        let (count, violations) = bodies("(when ready (go))");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_dolist_with_a_body() {
        let (_, violations) = bodies("(dolist (x items) (print x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_form_missing_its_prefix() {
        // (when) has no test; that is a malformed form, not an empty body.
        let (_, violations) = bodies("(when)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = bodies("(cond (x))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = bodies("(WHEN ready)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_form_nested_in_a_body() {
        let (_, violations) = bodies("(defun f (x) (when x))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(when ready)", Dialect::Clojure).expect("parse");
        let report = build_empty_body_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build empty body report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("body_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(when ready (go))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_head() {
        let report = report("(defun f (x)\n  (when x))\n");
        let finding = &report.findings[0];
        assert_eq!(finding.line, 2);
        assert_eq!(finding.kind(), "empty-body");
        assert_eq!(finding.json_fields(), vec![("head", json!("when"))]);
        assert_eq!(finding.text_columns(), vec!["head=when".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_body_form_scanned_not_only_the_flagged_ones() {
        let report = report("(when ready)\n(when ready (go))\n(dolist (x items))\n");
        assert_eq!(report.summary, vec![("body_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
