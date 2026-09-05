//! Common Lisp t-comparison detection: `(eq X t)`, `(eql X t)`, `(equal X t)`,
//! or `(equalp X t)` (with the literal `t` on either side). Comparing a value
//! against the symbol `t` only succeeds when the value is *exactly* `t` — but
//! Common Lisp predicates return *generalized* booleans (any non-nil value is
//! "true"), so `(eq (evenp n) t)` works by luck (because `evenp` happens to
//! return `t`) while `(eq (member x xs) t)` is always false even when `x` is a
//! member, because `member` returns the tail, not `t`. A test that means "is X
//! true" should be just `X` (or `(not (null X))`); one that means "is X exactly
//! the symbol T" is almost always a misunderstanding.
//!
//! This is the report-only counterpart to
//! [`crate::nil_comparison::domain`]: `(eq X nil)` is unambiguously
//! `(null X)` and is auto-fixed, but the right rewrite for `(eq X t)` depends on
//! intent (drop the comparison, or keep an intentional symbol test), so this
//! rule surfaces the smell without editing.
//!
//! Only the two-operand shape with a bare `t` symbol literal is flagged:
//!
//!   - All four object-equality predicates (`eq`/`eql`/`equal`/`equalp`) are
//!     matched. `=` is excluded (`(= x t)` is a type error).
//!   - The `t` may appear as either operand (`(eq t x)` too).
//!   - A degenerate `(eq t t)` (both operands `t`) is left to `self-comparison`.
//!   - Only the bare symbol `t` counts; a quoted `'t` (which carries a reader
//!     prefix) is left alone.
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

/// The canonical operator name for an object-equality predicate head, or `None`
/// otherwise. `=` is intentionally excluded (numeric, and a type error on `t`).
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

/// Whether `view` is the bare `t` symbol literal (no reader prefixes, so a
/// quoted `'t` is excluded).
fn is_t_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty()
        && atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

#[derive(Debug, Clone)]
pub struct TComparisonItem {
    /// The span of the whole `(eq X t)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`eq`/`eql`/`equal`/`equalp`).
    pub operator: &'static str,
}

impl Finding for TComparisonItem {
    /// The equality predicate, so `eq` and `equalp` are separable without
    /// parsing JSON.
    ///
    /// A closed set of four, already normalized to lowercase `&'static str` by
    /// [`equality_operator`] — the source casing is not retained, so this is
    /// the canonical name rather than whatever the file spelled.
    fn kind(&self) -> &'static str {
        self.operator
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    /// Nothing beyond the leading `kind`: the old text row's only column was
    /// `operator=…`, which the `kind` column now carries.
    fn text_columns(&self) -> Vec<String> {
        Vec::new()
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("operator", json!(self.operator))]
    }

    fn message(&self) -> String {
        format!(
            "{} against t matches only the symbol T, not any true value",
            self.operator
        )
    }
}

pub fn examine_comparison(
    view: &ExpressionView,
    comparison_form_count: &mut usize,
    violations: &mut Vec<TComparisonItem>,
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
    let (left_t, right_t) = (
        is_t_literal(&view.children[1]),
        is_t_literal(&view.children[2]),
    );

    // Exactly one operand must be the `t` literal; a both-`t` form is
    // degenerate and left to `self-comparison`.
    if left_t == right_t {
        return;
    }

    violations.push(TComparisonItem {
        span: view.span,
        operator,
    });
}

/// Collects every t comparison in one file, with the number of
/// eq/eql/equal/equalp forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_t_comparison_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<TComparisonItem>> {
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

    fn report(input: &str) -> FileFindings<TComparisonItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_t_comparison_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build t comparison report")
    }

    fn comparisons(input: &str) -> (u64, Vec<TComparisonItem>) {
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
    fn flags_eq_against_trailing_t() {
        let (count, violations) = comparisons("(eq x t)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn flags_t_first_operand() {
        let (_, violations) = comparisons("(eql t (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eql");
    }

    #[test]
    fn flags_equal_and_equalp() {
        assert_eq!(comparisons("(equal x t)").1.len(), 1);
        assert_eq!(comparisons("(equalp x t)").1.len(), 1);
    }

    #[test]
    fn does_not_flag_numeric_equal_sign() {
        let (count, violations) = comparisons("(= x t)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_both_t() {
        let (count, violations) = comparisons("(eq t t)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_without_a_t_operand() {
        let (_, violations) = comparisons("(eq x y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_quoted_t() {
        let (_, violations) = comparisons("(eq x 't)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_nil_comparison() {
        // nil-comparison owns (eq x nil); this rule must not also fire on it.
        let (_, violations) = comparisons("(eq x nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        let (_, violations) = comparisons("(eq x t y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_operator_and_t() {
        let (_, violations) = comparisons("(EQ x T)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn finds_a_nested_t_comparison() {
        let (_, violations) = comparisons("(when (eq (evenp n) t) (go))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(eq x t)", Dialect::Clojure).expect("parse");
        let report = build_t_comparison_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build t comparison report");
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
        let report = report("(defun ready? (x)\n  (eq (compute x) t))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "eq");
        assert_eq!(finding.json_fields(), vec![("operator", json!("eq"))]);
        assert!(finding.text_columns().is_empty());
        assert_eq!(
            finding.message(),
            "eq against t matches only the symbol T, not any true value"
        );
    }

    #[test]
    fn the_summary_counts_every_comparison_scanned_not_only_the_flagged_ones() {
        let report = report("(eq x t)\n(eql a b)\n(equal c d)\n");
        assert_eq!(report.summary, vec![("comparison_form_count", json!(3))]);
        assert_eq!(report.findings.len(), 1);
    }
}
