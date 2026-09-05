//! Common Lisp `prog2`-to-`progn` detection: a two-argument `(prog2 a b)`.
//!
//! `prog2` evaluates its forms left to right and returns the value(s) of the
//! *second*; with exactly two forms the second is also the last, so
//! `(prog2 a b)` returns the same value(s) as `(progn a b)` — same evaluation
//! order, same result. `progn` states the sequencing without the (now
//! irrelevant) "return the second form" twist of `prog2`.
//!
//! Only the exact two-form shape is matched. A `(prog2 a b c …)` returns `b`
//! (the second form), which `progn` (returning the last) cannot express, so it
//! is left alone, as is a one-form `(prog2 a)` and a reader-conditional body.
//!
//! The fix rewrites the operator token `prog2` to `progn`, leaving the two forms
//! byte-identical, so the rule is auto-fixable.
//!
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};
use serde_json::{Value, json};

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct Prog2ToPrognItem {
    /// The span of the whole `(prog2 a b)` form.
    pub span: ByteSpan,
    /// The span of the `prog2` operator token (rewritten to `progn`).
    pub head_span: ByteSpan,
}

impl Finding for Prog2ToPrognItem {
    /// The rule's own name. There is exactly one shape this rule flags — the
    /// two-form `(prog2 a b)` — so there is no variant to separate.
    fn kind(&self) -> &'static str {
        "prog2-to-progn"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Empty: the old text row carried nothing beyond the path and offset, both
    /// of which the envelope supplies.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// `head_span` is the operator token the fix rewrites. It is on the report
    /// because the hand-written renderer published it, and a consumer applying
    /// the same edit itself needs it.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![(
            "head_span",
            json!({
                "start": self.head_span.start().get(),
                "end": self.head_span.end().get(),
            }),
        )]
    }

    fn message(&self) -> String {
        "a two-form prog2 is just progn; (prog2 a b) is (progn a b)".to_owned()
    }
}

pub fn examine(
    view: &ExpressionView,
    prog2_form_count: &mut usize,
    violations: &mut Vec<Prog2ToPrognItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("prog2") {
        return;
    }
    *prog2_form_count += 1;

    // children: [prog2, a, b] — require exactly two body forms.
    if view.children.len() != 3 {
        return;
    }
    if view.children[1..].iter().any(is_reader_conditional) {
        return;
    }

    violations.push(Prog2ToPrognItem {
        span: view.span,
        head_span: view.children[0].span,
    });
}

/// Collects every two-form `(prog2 a b)` in one file, with the number of
/// `prog2` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_prog2_to_progn_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<Prog2ToPrognItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("prog2_form_count", json!(0))],
        ));
    }

    let mut prog2_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut prog2_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("prog2_form_count", json!(prog2_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<Prog2ToPrognItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_prog2_to_progn_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build prog2 to progn report")
    }

    fn prog2s(input: &str) -> (u64, Vec<Prog2ToPrognItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "prog2_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("prog2_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_two_form_prog2() {
        let source = "(prog2 (setup) (run))";
        let (count, violations) = prog2s(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].head_span), "prog2");
    }

    #[test]
    fn does_not_flag_three_form_prog2() {
        // (prog2 a b c) returns b, which progn cannot express.
        assert!(prog2s("(prog2 a b c)").1.is_empty());
    }

    #[test]
    fn does_not_flag_one_form_prog2() {
        assert!(prog2s("(prog2 a)").1.is_empty());
    }

    #[test]
    fn case_folds_head() {
        let (_, violations) = prog2s("(PROG2 a b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = prog2s("(defun f (a b) (prog2 a b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(prog2 a b)", Dialect::Clojure).expect("parse");
        let report = build_prog2_to_progn_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build prog2 to progn report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("prog2_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(prog2 a b c)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator_span() {
        let source = "(defun f (a b)\n  (prog2 a b))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "prog2-to-progn");
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.json_fields(),
            vec![(
                "head_span",
                json!({
                    "start": finding.head_span.start().get(),
                    "end": finding.head_span.end().get(),
                })
            )]
        );
        assert_eq!(slice(source, finding.head_span), "prog2");
    }

    #[test]
    fn the_summary_counts_every_prog2_scanned_not_only_the_flagged_ones() {
        let report = report("(prog2 a b)\n(prog2 a b c)\n");
        assert_eq!(report.summary, vec![("prog2_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
