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
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

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
    pub path: PathBuf,
    /// The span of the whole `(= X 0)` form.
    pub span: ByteSpan,
    /// The suggested predicate (`zerop`/`plusp`/`minusp`).
    pub predicate: &'static str,
    /// The span of the non-zero operand `X` (for reconstructing the fix).
    pub operand_span: ByteSpan,
}

#[derive(Debug)]
pub struct SignComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<SignComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SignComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl SignComparisonPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SignComparisonPolicy {
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
        path: path.to_path_buf(),
        span: view.span,
        predicate,
        operand_span: operand.span,
    });
}

/// Collects every sign comparison across a whole file, along with the total
/// number of `=`/`>`/`<` forms scanned.
pub fn collect_sign_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SignComparisonItem>)> {
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

pub fn summarize_sign_comparisons(
    comparison_form_count: usize,
    violations: Vec<SignComparisonItem>,
) -> SignComparisonSummary {
    SignComparisonSummary {
        comparison_form_count,
        violations,
    }
}

pub fn evaluate_sign_comparison_policy(
    options: SignComparisonPolicyOptions,
    summary: &SignComparisonSummary,
) -> SignComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SignComparisonPolicy {
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

    fn comparisons(input: &str) -> (usize, Vec<SignComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_sign_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect sign comparisons")
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
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(= n 0)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_sign_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect sign comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = comparisons("(= n 0)");
        let summary = summarize_sign_comparisons(count, items);

        let quiet =
            evaluate_sign_comparison_policy(SignComparisonPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_sign_comparison_policy(SignComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
