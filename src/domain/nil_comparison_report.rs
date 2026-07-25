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
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

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
    pub path: PathBuf,
    /// The span of the whole `(eq X nil)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`eq`/`eql`/`equal`/`equalp`).
    pub operator: &'static str,
    /// The span of the non-nil operand `X` (lets a fix reconstruct `(null X)`).
    pub operand_span: ByteSpan,
}

#[derive(Debug)]
pub struct NilComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<NilComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct NilComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl NilComparisonPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct NilComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
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
        path: path.to_path_buf(),
        span: view.span,
        operator,
        operand_span: operand.span,
    });
}

/// Collects every nil comparison across a whole file, along with the total
/// number of eq/eql/equal/equalp forms scanned.
pub fn collect_nil_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<NilComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, path, &mut comparison_form_count, &mut violations)
        });
    }
    Ok((comparison_form_count, violations))
}

pub fn summarize_nil_comparisons(
    comparison_form_count: usize,
    violations: Vec<NilComparisonItem>,
) -> NilComparisonSummary {
    NilComparisonSummary {
        comparison_form_count,
        violations,
    }
}

pub fn evaluate_nil_comparison_policy(
    options: NilComparisonPolicyOptions,
    summary: &NilComparisonSummary,
) -> NilComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    NilComparisonPolicy {
        fail_on_violation: options.fail_on_violation(),
        comparison_form_count: summary.comparison_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparisons(input: &str) -> (usize, Vec<NilComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_nil_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect nil comparisons")
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

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(eq x nil)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_nil_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect nil comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = comparisons("(eq x nil)");
        let summary = summarize_nil_comparisons(count, items);

        let quiet =
            evaluate_nil_comparison_policy(NilComparisonPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_nil_comparison_policy(NilComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
