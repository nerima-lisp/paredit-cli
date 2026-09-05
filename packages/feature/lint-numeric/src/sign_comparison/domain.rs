//! Common Lisp sign-comparison detection: a two-argument `=`/`>`/`<` comparison
//! against the literal `0`, which the language provides a dedicated predicate
//! for — `(= x 0)` is `(zerop x)`, `(> x 0)` is `(plusp x)`, `(< x 0)` is
//! `(minusp x)`. Those predicates are *defined* as exactly these comparisons, so
//! the rewrite is exact (same value, same numeric type contract) and the
//! predicate states the intent ("is this zero / positive / negative") directly.
//!
//! The suggested predicate depends on the operator *and* which side the `0` is
//! on, because `>`/`<` are not symmetric:
//!
//!   - `(= x 0)` / `(= 0 x)`  → `(zerop x)`
//!   - `(> x 0)`  → `(plusp x)`      `(> 0 x)`  → `(minusp x)`
//!   - `(< x 0)`  → `(minusp x)`     `(< 0 x)`  → `(plusp x)`
//!
//! Only the two-operand shape with exactly one bare `0` literal is flagged;
//! `>=`/`<=`/`/=` (which have no single-word predicate), a three-argument
//! comparison, a `0.0`/`#x0` spelling, a degenerate `(= 0 0)`, and a
//! reader-conditional operand are all left alone.
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

/// Whether `view` is the bare integer `0` literal (no reader prefixes, so `#x0`
/// and a prefixed `,0` are excluded; `0.0` is a different spelling and excluded).
fn is_zero_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("0")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// comparison containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The dedicated predicate for operator `op` with the `0` on the given side.
fn sign_predicate(op: &str, zero_on_left: bool) -> Option<&'static str> {
    match op {
        "=" => Some("zerop"),
        ">" if zero_on_left => Some("minusp"), // (> 0 x) is x < 0
        ">" => Some("plusp"),                  // (> x 0)
        "<" if zero_on_left => Some("plusp"),  // (< 0 x) is x > 0
        "<" => Some("minusp"),                 // (< x 0)
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct SignComparisonItem {
    /// The span of the whole `(= X 0)` form.
    pub span: ByteSpan,
    /// The suggested predicate (`zerop`/`plusp`/`minusp`).
    pub predicate: &'static str,
    /// The span of the non-zero operand `X`.
    ///
    pub operand_span: ByteSpan,
}

impl Finding for SignComparisonItem {
    /// The predicate this comparison should have been written as, so
    /// `zerop`, `plusp` and `minusp` are separable without parsing JSON.
    ///
    /// The operator would not do: `=`, `>` and `<` are punctuation, and the
    /// same operator maps to two different predicates depending on which side
    /// the `0` is on. The predicate is the closed, named set here.
    fn kind(&self) -> &'static str {
        self.predicate
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the leading `kind`: the old text row's only column was
    /// `predicate=…`, which the `kind` column now carries.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("predicate", json!(self.predicate))]
    }

    fn message(&self) -> String {
        format!(
            "comparison against 0 has a dedicated predicate; use {}",
            self.predicate
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<SignComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !matches!(head, "=" | ">" | "<") {
        return;
    }
    *comparison_form_count += 1;

    // children[0] is the operator; require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let left = &view.children[1];
    let right = &view.children[2];
    if is_reader_conditional(left) || is_reader_conditional(right) {
        return;
    }
    let (left_zero, right_zero) = (is_zero_literal(left), is_zero_literal(right));

    // Exactly one operand must be the literal `0`; the other is `X`.
    let (operand, zero_on_left) = match (left_zero, right_zero) {
        (true, false) => (right, true),
        (false, true) => (left, false),
        _ => return,
    };
    let Some(predicate) = sign_predicate(head, zero_on_left) else {
        return;
    };

    violations.push(SignComparisonItem {
        span: view.span,
        predicate,
        operand_span: operand.span,
    });
}

/// Collects every sign comparison in one file, with the number of `=`/`>`/`<`
/// forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_sign_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<SignComparisonItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("comparison_form_count", json!(0))],
        ));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, &mut comparison_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("comparison_form_count", json!(comparison_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<SignComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_sign_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build sign comparison report")
    }

    fn comparisons(input: &str) -> (u64, Vec<SignComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "comparison_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("comparison_form_count in the summary");
        (count, report.findings)
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn equals_zero_is_zerop() {
        let source = "(= n 0)";
        let (count, violations) = comparisons(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].predicate, "zerop");
        assert_eq!(slice(source, violations[0].operand_span), "n");
    }

    #[test]
    fn equals_zero_is_symmetric() {
        let (_, violations) = comparisons("(= 0 n)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].predicate, "zerop");
    }

    #[test]
    fn greater_than_zero_is_plusp() {
        let (_, violations) = comparisons("(> count 0)");
        assert_eq!(violations[0].predicate, "plusp");
    }

    #[test]
    fn zero_greater_than_x_is_minusp() {
        let (_, violations) = comparisons("(> 0 balance)");
        assert_eq!(violations[0].predicate, "minusp");
    }

    #[test]
    fn less_than_zero_is_minusp() {
        let (_, violations) = comparisons("(< delta 0)");
        assert_eq!(violations[0].predicate, "minusp");
    }

    #[test]
    fn zero_less_than_x_is_plusp() {
        let (_, violations) = comparisons("(< 0 amount)");
        assert_eq!(violations[0].predicate, "plusp");
    }

    #[test]
    fn preserves_a_compound_operand() {
        let source = "(= (length xs) 0)";
        let (_, violations) = comparisons(source);
        assert_eq!(violations[0].predicate, "zerop");
        assert_eq!(slice(source, violations[0].operand_span), "(length xs)");
    }

    #[test]
    fn does_not_flag_ge_le_or_ne() {
        assert!(comparisons("(>= x 0)").1.is_empty());
        assert!(comparisons("(<= x 0)").1.is_empty());
        assert!(comparisons("(/= x 0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_float_zero() {
        let (_, violations) = comparisons("(= x 0.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_zero_literal() {
        let (count, violations) = comparisons("(= x 5)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_both_zero() {
        let (_, violations) = comparisons("(= 0 0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        let (_, violations) = comparisons("(= x 0 y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_sign_comparison() {
        let (_, violations) = comparisons("(when (> remaining 0) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].predicate, "plusp");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(= n 0)", Dialect::Clojure).expect("parse");
        let report = build_sign_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build sign comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(= n 0)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_predicate() {
        let report = report("(defun empty? (n)\n  (= n 0))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "zerop");
        assert_eq!(finding.json_fields(), vec![("predicate", json!("zerop"))]);
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "comparison against 0 has a dedicated predicate; use zerop"
        );
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(= x 0)\n(= x 5)\n(> a b)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
