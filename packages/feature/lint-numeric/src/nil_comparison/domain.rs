//! Common Lisp nil-comparison detection: `(eq X nil)`, `(eql X nil)`,
//! `(equal X nil)`, or `(equalp X nil)` (with `nil` on either side). Every one
//! of these equality predicates returns true exactly when `X` is `nil`, which is
//! precisely what the dedicated `null` predicate tests — so `(eq x nil)` is just
//! `(null x)`. The idiomatic `null` form states the intent (a nil/empty-list
//! test) directly instead of dressing it up as an object-identity comparison.
//!
//! Only the two-operand shape with a bare `nil` symbol literal is flagged:
//!
//!   - All four object-equality predicates (`eq`/`eql`/`equal`/`equalp`) agree
//!     with `null` on a `nil` argument, so all four are matched. `=` is *not*
//!     included: `(= x nil)` is a type error (`nil` is not a number).
//!   - The `nil` may appear as either operand (`(eq nil x)` too).
//!   - A degenerate `(eq nil nil)` (both operands `nil`) is left to the
//!     `self-comparison` rule and not flagged here.
//!   - Only the bare symbol `nil` counts; a quoted `'nil` (which carries a
//!     reader prefix) is left alone.
//!
//! The fix rewrites the whole form as `(null X)`, copying `X`'s exact source
//! bytes, so the rule is auto-fixable.
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

/// The canonical operator name for an object-equality predicate head, or `None`
/// otherwise. `=` is intentionally excluded (numeric, and a type error on nil).
fn equality_operator(head: &str) -> Option<&'static str> {
    if head.eq_ignore_ascii_case("eq") {
        Some("eq")
    } else if head.eq_ignore_ascii_case("eql") {
        Some("eql")
    } else if head.eq_ignore_ascii_case("equal") {
        Some("equal")
    } else if head.eq_ignore_ascii_case("equalp") {
        Some("equalp")
    } else {
        None
    }
}

/// Whether `view` is the bare `nil` symbol literal (no reader prefixes, so a
/// quoted `'nil` is excluded).
fn is_nil_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
}

#[derive(Debug, Clone)]
pub struct NilComparisonItem {
    /// The span of the whole `(eq X nil)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`eq`/`eql`/`equal`/`equalp`).
    pub operator: &'static str,
    /// The span of the non-nil operand `X` (lets a fix reconstruct `(null X)`).
    ///
    /// The rewrite's input, not the report's: the lint rule copies it into the
    /// `(null X)` form, and the command never prints it.
    pub operand_span: ByteSpan,
}

impl Finding for NilComparisonItem {
    /// The predicate that was used, which is already one of four lowercase
    /// names. They are not interchangeable to a reader — `equalp` against nil
    /// is a heavier mistake than `eq` against nil — and a consumer filtering on
    /// one of them is asking a real question.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("operator={}", self.operator)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    /// The same sentence the `nil-comparison` lint rule writes, so a SARIF or
    /// JUnit consumer reading both sees one finding described one way.
    fn message(&self) -> String {
        format!("{} against nil is a null test; use (null X)", self.operator)
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<NilComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = equality_operator(head) else {
        return;
    };
    *comparison_form_count += 1;

    // children[0] is the operator; require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let left = &view.children[1];
    let right = &view.children[2];
    let (left_nil, right_nil) = (is_nil_literal(left), is_nil_literal(right));

    // Exactly one operand must be the nil literal; the other is `X`. A
    // both-nil form is degenerate and left to `self-comparison`.
    let operand = match (left_nil, right_nil) {
        (true, false) => right,
        (false, true) => left,
        _ => return,
    };

    violations.push(NilComparisonItem {
        span: view.span,
        operator,
        operand_span: operand.span,
    });
}

/// Collects every nil comparison in one file, with the number of
/// eq/eql/equal/equalp forms scanned as the denominator beside them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no comparison against nil here" for
/// Common Lisp and "nothing was looked for" for Fennel, and the two read
/// identically without the flag.
pub fn build_nil_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<NilComparisonItem>> {
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

    fn report(input: &str) -> FileFindings<NilComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_nil_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build nil comparison report")
    }

    /// The `(comparison_form_count, violations)` pair the report is built from.
    fn comparisons(input: &str) -> (u64, Vec<NilComparisonItem>) {
        let report = report(input);
        let count = report
            .summary
            .iter()
            .find(|(name, _)| *name == "comparison_form_count")
            .and_then(|(_, value)| value.as_u64())
            .expect("comparison_form_count in the summary");
        (count, report.findings)
    }

    #[test]
    fn flags_eq_against_trailing_nil() {
        let (count, violations) = comparisons("(eq x nil)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn flags_nil_first_operand() {
        let (_, violations) = comparisons("(eql nil (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
    }

    #[test]
    fn flags_equal_and_equalp() {
        assert_eq!(comparisons("(equal x nil)").1.len(), 1);
        assert_eq!(comparisons("(equalp x nil)").1.len(), 1);
    }

    #[test]
    fn operand_span_covers_only_the_non_nil_operand() {
        // Deleting/rewriting must reconstruct `(null (foo bar))`.
        let source = "(eq (foo bar) nil)";
        let (_, violations) = comparisons(source);
        let operand = violations[0].operand_span;
        assert_eq!(
            source.get(operand.start().get()..operand.end().get()),
            Some("(foo bar)")
        );
    }

    #[test]
    fn does_not_flag_numeric_equal_sign() {
        let (count, violations) = comparisons("(= x nil)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_both_nil() {
        let (count, violations) = comparisons("(eq nil nil)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_without_a_nil_operand() {
        let (_, violations) = comparisons("(eq x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_quoted_nil() {
        let (_, violations) = comparisons("(eq x 'nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        let (_, violations) = comparisons("(eq x nil y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_operator_and_nil() {
        let (_, violations) = comparisons("(EQ x NIL)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn finds_a_nested_nil_comparison() {
        let (_, violations) = comparisons("(when (eq item nil) (skip))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    /// A dialect this rule cannot read must say so, rather than return the
    /// empty finding list a clean Common Lisp file returns.
    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(eq x nil)", Dialect::Clojure).expect("parse");
        let report = build_nil_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build nil comparison report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("comparison_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(eq x y)").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_operator() {
        let report = report("(defun done? (x)\n  (eq x nil))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eq");
        assert_eq!(finding.json_fields(), vec![("operator", json!("eq"))]);
        assert_eq!(finding.text_columns(), vec!["operator=eq".to_owned()]);
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(eq x nil)\n(eq a b)\n(equalp y nil)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 2);
    }
}
