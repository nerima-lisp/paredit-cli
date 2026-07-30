//! Common Lisp string-case-fold detection: a `string=` comparing two operands
//! that are each case-folded the same way — `(string= (string-downcase a)
//! (string-downcase b))` (or both `string-upcase`). Case-folding both sides and
//! then comparing case-sensitively (`string=`) is exactly a case-insensitive
//! comparison, which `string-equal` performs directly (and without allocating
//! the two folded copies). So the form is `(string-equal a b)`.
//!
//! Both operands must use the *same* case operation (`string-downcase` on both,
//! or `string-upcase` on both); a mixed pair (`(string= (string-downcase a)
//! (string-upcase b))`) compares different casings and is *not* case-insensitive,
//! so it is left alone. Only the exact two-operand `(string= X Y)` shape is
//! matched (a `:start`/`:end`-keyworded or three-argument comparison is left
//! alone), and a reader-conditional inner argument is left alone.
//!
//! The fix rewrites the form as `(string-equal a b)`, copying the two inner
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

/// The non-destructive string case operations that fold to a canonical case.
const CASE_OPS: [&str; 2] = ["string-downcase", "string-upcase"];

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// If `view` is a single-argument `(string-downcase x)` / `(string-upcase x)`
/// call, returns `(lowercased-op, x)`.
fn case_folded(view: &ExpressionView) -> Option<(String, &ExpressionView)> {
    if !is_paren_list(view) {
        return None;
    }
    let head = list_head(view)?;
    let op = CASE_OPS.iter().find(|op| head.eq_ignore_ascii_case(op))?;
    // children: [op, arg] — exactly one argument.
    if view.children.len() != 2 {
        return None;
    }
    Some(((*op).to_owned(), &view.children[1]))
}

#[derive(Debug, Clone)]
pub struct StringCaseFoldItem {
    /// The span of the whole `(string= …)` form.
    pub span: ByteSpan,
    /// The span of the first folded operand's argument `a`.
    pub left_span: ByteSpan,
    /// The span of the second folded operand's argument `b`.
    pub right_span: ByteSpan,
}

impl Finding for StringCaseFoldItem {
    /// One tag for every finding: this report has a single shape to describe, a
    /// case-insensitive comparison spelled as a case-sensitive one over two
    /// folded copies.
    ///
    /// Which case operation was folded with (`string-downcase` or
    /// `string-upcase`) is not it: both sides must agree for the form to be
    /// flagged at all, and the rewrite is the same either way.
    fn kind(&self) -> &'static str {
        "string-case-fold"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// None: the old text row carried the path and the offset, both of which
    /// the envelope prints itself.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    /// The two operand spans are the rewrite's input, but the old JSON
    /// published both, so a consumer reconstructing `(string-equal a b)` from
    /// this report keeps them.
    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "left_span",
                json!({
                    "start": self.left_span.start().get(),
                    "end": self.left_span.end().get(),
                }),
            ),
            (
                "right_span",
                json!({
                    "start": self.right_span.start().get(),
                    "end": self.right_span.end().get(),
                }),
            ),
        ]
    }

    /// The same sentence the `string-case-fold` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        "case-folding both sides of string= is case-insensitive; use string-equal".to_owned()
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine(
    view: &ExpressionView,
    compare_form_count: &mut usize,
    violations: &mut Vec<StringCaseFoldItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !head.eq_ignore_ascii_case("string=") {
        return;
    }
    *compare_form_count += 1;

    // children: [string=, left, right] — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let Some((left_op, left_arg)) = case_folded(&view.children[1]) else {
        return;
    };
    let Some((right_op, right_arg)) = case_folded(&view.children[2]) else {
        return;
    };
    // Both sides must fold the same way to be a case-insensitive comparison.
    if !left_op.eq_ignore_ascii_case(&right_op) {
        return;
    }
    if is_reader_conditional(left_arg) || is_reader_conditional(right_arg) {
        return;
    }

    violations.push(StringCaseFoldItem {
        span: view.span,
        left_span: left_arg.span,
        right_span: right_arg.span,
    });
}

/// Collects every `(string= (string-downcase a) (string-downcase b))` (or
/// upcase) in one file, with the number of `string=` forms scanned as the
/// denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no folded comparison here" for Common
/// Lisp and "nothing was looked for" for Clojure, and the two read identically
/// without the flag.
pub fn build_string_case_fold_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<StringCaseFoldItem>> {
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

    fn report(input: &str) -> FileFindings<StringCaseFoldItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_string_case_fold_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build string case fold report")
    }

    /// The `(compare_form_count, violations)` pair the report is built from.
    fn cmps(input: &str) -> (u64, Vec<StringCaseFoldItem>) {
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
        let source = "(string= (string-downcase a) (string-downcase b))";
        let (count, violations) = cmps(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].left_span), "a");
        assert_eq!(slice(source, violations[0].right_span), "b");
    }

    #[test]
    fn flags_upcase_both_sides() {
        let (_, violations) = cmps("(string= (string-upcase a) (string-upcase b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn preserves_compound_arguments() {
        let source = "(string= (string-downcase (name x)) (string-downcase key))";
        let (_, violations) = cmps(source);
        assert_eq!(slice(source, violations[0].left_span), "(name x)");
        assert_eq!(slice(source, violations[0].right_span), "key");
    }

    #[test]
    fn does_not_flag_mixed_case_ops() {
        assert!(
            cmps("(string= (string-downcase a) (string-upcase b))")
                .1
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_one_folded_side() {
        assert!(cmps("(string= (string-downcase a) b)").1.is_empty());
    }

    #[test]
    fn does_not_flag_keyworded_comparison() {
        // (string= X Y :start1 0) is a four-argument form, not the bare shape.
        assert!(
            cmps("(string= (string-downcase a) (string-downcase b) :start1 0)")
                .1
                .is_empty()
        );
    }

    #[test]
    fn case_folds_head_and_ops() {
        let (_, violations) = cmps("(STRING= (STRING-DOWNCASE a) (STRING-DOWNCASE b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_nested() {
        let (_, violations) = cmps("(when (string= (string-downcase a) (string-downcase b)) (go))");
        assert_eq!(violations.len(), 1);
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(string= (string-downcase a) (string-downcase b))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_string_case_fold_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build string case fold report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("compare_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(string= a b)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_both_operand_spans() {
        let report =
            report("(defun f (a b)\n  (string= (string-downcase a) (string-downcase b)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "string-case-fold");
        assert_eq!(
            finding.json_fields(),
            vec![
                (
                    "left_span",
                    json!({
                        "start": finding.left_span.start().get(),
                        "end": finding.left_span.end().get(),
                    })
                ),
                (
                    "right_span",
                    json!({
                        "start": finding.right_span.start().get(),
                        "end": finding.right_span.end().get(),
                    })
                ),
            ]
        );
        assert!(finding.text_columns().is_empty());
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(string= (string-downcase a) (string-downcase b))\n(string= a b)\n(string= (string-upcase a) (string-upcase b))\n",
        );
        assert_eq!(report.summary, vec![("compare_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
