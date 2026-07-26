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
//! [`crate::domain::nil_comparison_report`]: `(eq X nil)` is unambiguously
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
    pub path: PathBuf,
    /// The span of the whole `(eq X t)` form.
    pub span: ByteSpan,
    /// The operator, lowercased (`eq`/`eql`/`equal`/`equalp`).
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct TComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<TComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct TComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl TComparisonPolicyOptions {
    #[must_use]
    pub const fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct TComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub(crate) fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
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
        path: path.to_path_buf(),
        span: view.span,
        operator,
    });
}

/// Collects every t comparison across a whole file, along with the total number
/// of eq/eql/equal/equalp forms scanned.
pub fn collect_t_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<TComparisonItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut comparison_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_comparison(subview, path, &mut comparison_form_count, &mut violations);
        });
    }
    Ok((comparison_form_count, violations))
}

#[must_use]
pub const fn summarize_t_comparisons(
    comparison_form_count: usize,
    violations: Vec<TComparisonItem>,
) -> TComparisonSummary {
    TComparisonSummary {
        comparison_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_t_comparison_policy(
    options: TComparisonPolicyOptions,
    summary: &TComparisonSummary,
) -> TComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    TComparisonPolicy {
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

    fn comparisons(input: &str) -> (usize, Vec<TComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_t_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect t comparisons")
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
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(eq x t)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_t_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect t comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = comparisons("(eq x t)");
        let summary = summarize_t_comparisons(count, items);

        let quiet = evaluate_t_comparison_policy(TComparisonPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_t_comparison_policy(TComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
