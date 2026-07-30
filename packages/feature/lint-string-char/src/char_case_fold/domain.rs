//! Common Lisp char-case-fold detection: a `char=` comparing two operands that
//! are each case-folded the same way — `(char= (char-downcase a) (char-downcase
//! b))` (or both `char-upcase`). Case-folding both characters and then comparing
//! case-sensitively (`char=`) is exactly a case-insensitive comparison, which
//! `char-equal` performs directly. So the form is `(char-equal a b)`.
//!
//! Both operands must use the *same* case operation (`char-downcase` on both, or
//! `char-upcase` on both); a mixed pair is not case-insensitive and is left
//! alone. Only the exact two-operand `(char= X Y)` shape is matched (a three-or-
//! more-argument comparison is left alone), and a reader-conditional inner
//! argument is left alone.
//!
//! The fix rewrites the form as `(char-equal a b)`, copying the two inner
//! arguments verbatim, so the rule is auto-fixable.
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

/// The character case operations that fold to a canonical case.
const CASE_OPS: [&str; 2] = ["char-downcase", "char-upcase"];

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `view` is a single-argument `(char-downcase x)` / `(char-upcase x)` call,
/// returns `(lowercased-op, x)`.
fn case_folded(view: &ExpressionView) -> Option<(String, &ExpressionView)> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    let op = CASE_OPS.iter().find(|op| head.eq_ignore_ascii_case(op))?;
    if view.children.len() != 2 {
        return None;
    }
    Some(((*op).to_owned(), &view.children[1]))
}

#[derive(Debug, Clone)]
pub struct CharCaseFoldItem {
    /// The span of the whole `(char= …)` form.
    pub span: ByteSpan,
    /// The span of the first folded operand's argument `a`.
    pub left_span: ByteSpan,
    /// The span of the second folded operand's argument `b`.
    pub right_span: ByteSpan,
}

impl Finding for CharCaseFoldItem {
    /// The rule's own name: a case-folded `char=` has no sub-classes to
    /// separate, since both operands are folded the same way by construction.
    fn kind(&self) -> &'static str {
        "char-case-fold"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The two operand spans, which the old report already published and a
    /// caller synthesizing the `(char-equal a b)` rewrite reads.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("left_span", span_json(self.left_span)),
            ("right_span", span_json(self.right_span)),
        ]
    }

    /// The same sentence the `char-case-fold` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "case-folding both sides of char= is case-insensitive; use char-equal".to_owned()
    }
}

fn span_json(span: ByteSpan) -> Value {
    json!({ "start": span.start().get(), "end": span.end().get() })
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    compare_form_count: &mut usize,
    violations: &mut Vec<CharCaseFoldItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("char=") {
        return;
    }
    *compare_form_count += 1;

    // children: [char=, left, right] — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let Some((left_op, left_arg)) = case_folded(&view.children[1]) else {
        return;
    };
    let Some((right_op, right_arg)) = case_folded(&view.children[2]) else {
        return;
    };
    if !left_op.eq_ignore_ascii_case(&right_op) {
        return;
    }
    if is_reader_conditional(left_arg) || is_reader_conditional(right_arg) {
        return;
    }

    violations.push(CharCaseFoldItem {
        span: view.span,
        left_span: left_arg.span,
        right_span: right_arg.span,
    });
}

/// Collects every `(char= (char-downcase a) (char-downcase b))` (or upcase) in
/// one file, with the number of `char=` forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no case-folded comparison here" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_char_case_fold_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<CharCaseFoldItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("compare_form_count", json!(0))],
        ));
    }

    let mut compare_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine(subview, &mut compare_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("compare_form_count", json!(compare_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<CharCaseFoldItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_char_case_fold_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build char case fold report")
    }

    /// The `(compare_form_count, violations)` pair the report is built from.
    fn cmps(input: &str) -> (u64, Vec<CharCaseFoldItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "compare_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("compare_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_downcase_both_sides() {
        let source = "(char= (char-downcase a) (char-downcase b))";
        let (count, violations) = cmps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].left_span), "a");
        assert_eq!(slice(source, violations[0].right_span), "b");
    }

    #[test]
    fn flags_upcase_both_sides() {
        let (_, violations) = cmps("(char= (char-upcase a) (char-upcase b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_mixed_case_ops() {
        assert!(
            cmps("(char= (char-downcase a) (char-upcase b))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_one_folded_side() {
        assert!(cmps("(char= (char-downcase a) b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_three_argument_compare() {
        assert!(
            cmps("(char= (char-downcase a) (char-downcase b) (char-downcase c))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn case_folds_head_and_ops() {
        let (_, violations) = cmps("(CHAR= (CHAR-DOWNCASE a) (CHAR-DOWNCASE b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cmps("(when (char= (char-downcase a) (char-downcase b)) (go))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(char= (char-downcase a) (char-downcase b))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_char_case_fold_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build char case fold report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("compare_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(char= a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operand_spans() {
        let source = "(defun f (a b)\n  (char= (char-downcase a) (char-downcase b)))\n";
        let report = report(source);
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "char-case-fold");
        assert_eq!(
            finding.json_fields(),
            vec![
                ("left_span", span_json(finding.left_span)),
                ("right_span", span_json(finding.right_span)),
            ]
        );
        assert_eq!(slice(source, finding.left_span), "a");
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(char= (char-downcase a) (char-downcase b))\n(char= a b)\n");
        assert_eq!(report.summary, vec![("compare_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
