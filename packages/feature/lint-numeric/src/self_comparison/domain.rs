//! Common Lisp self-comparison detection: a comparison call whose two
//! operands are structurally identical — `(eq x x)`, `(< a a)`,
//! `(equal (f y) (f y))`. An equality predicate applied to a value and
//! itself is trivially true, and an ordering predicate is trivially false,
//! so the test is dead: almost always one operand was meant to differ. This
//! is the Common Lisp analog of the `eq_op` lint.
//!
//! Covered operators are the equality predicates (`eq`, `eql`, `equal`,
//! `equalp`, `string=`, `char=`) and the strict/loose ordering predicates
//! (`<`, `>`, `<=`, `>=`, `string<`, `string>`, `char<`, `char>`). The
//! numeric `=` and `/=` are deliberately excluded: `(/= x x)` is a real (if
//! rare) idiom for detecting a floating-point NaN, where a value genuinely
//! is not `=` to itself, and flagging it would be a false positive.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`] and the reader-aware
//! structural comparison from [`paredit_core_syntax::expression_equality`], so
//! `(eq x X)` counts (symbols fold case) while `(eq x y)` does not.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::{expressions_structurally_equal, render_expression};
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

const COMPARISON_HEADS: [&str; 14] = [
    "eq", "eql", "equal", "equalp", "string=", "char=", "<", ">", "<=", ">=", "string<", "string>",
    "char<", "char>",
];

#[derive(Debug, Clone)]
pub struct SelfComparisonItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub operand: String,
}

#[derive(Debug)]
pub struct SelfComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<SelfComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SelfComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl SelfComparisonPolicyOptions {
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
pub struct SelfComparisonPolicy {
    pub fail_on_violation: bool,
    pub comparison_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine_comparison(
    view: &ExpressionView,
    path: &Path,
    comparison_form_count: &mut usize,
    violations: &mut Vec<SelfComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !COMPARISON_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *comparison_form_count += 1;

    // Report the first pair of structurally-equal operands (after the
    // operator); a call is one violation, not one per repeated operand.
    let operands: Vec<&ExpressionView> = view.children.iter().skip(1).collect();
    for anchor in 0..operands.len() {
        for candidate in (anchor + 1)..operands.len() {
            if expressions_structurally_equal(operands[anchor], operands[candidate]) {
                violations.push(SelfComparisonItem {
                    path: path.to_path_buf(),
                    span: view.span,
                    operator: head.to_owned(),
                    operand: render_expression(operands[anchor]),
                });
                return;
            }
        }
    }
}

/// Collects every comparison call with two structurally-equal operands
/// across a whole file, along with the total number of comparison calls
/// scanned.
pub fn collect_self_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SelfComparisonItem>)> {
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
pub const fn summarize_self_comparisons(
    comparison_form_count: usize,
    violations: Vec<SelfComparisonItem>,
) -> SelfComparisonSummary {
    SelfComparisonSummary {
        comparison_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_self_comparison_policy(
    options: SelfComparisonPolicyOptions,
    summary: &SelfComparisonSummary,
) -> SelfComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SelfComparisonPolicy {
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

    fn comparisons(input: &str) -> (usize, Vec<SelfComparisonItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_self_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect self comparisons")
    }

    #[test]
    fn flags_eq_of_a_variable_with_itself() {
        let (comparison_form_count, violations) = comparisons("(eq x x)");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "eq");
        assert_eq!(violations[0].operand, "x");
    }

    #[test]
    fn flags_an_ordering_predicate_of_identical_operands() {
        let (_, violations) = comparisons("(< (score a) (score a))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operand, "(score a)");
    }

    #[test]
    fn folds_symbol_case_between_operands() {
        let (_, violations) = comparisons("(equal foo FOO)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_distinct_operands() {
        let (comparison_form_count, violations) = comparisons("(eq x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_numeric_equality_used_as_a_nan_check() {
        // `(/= x x)` and `(= x x)` are excluded because of the NaN idiom.
        let (comparison_form_count, violations) = comparisons("(and (= x x) (/= y y))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) = comparisons("(defun f (x) (when (eql x x) 1))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(eq x x)").expect("parse input");
        let (comparison_form_count, violations) =
            collect_self_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect self comparisons");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (comparison_form_count, items) = comparisons("(eq x x)");
        let summary = summarize_self_comparisons(comparison_form_count, items);

        let quiet =
            evaluate_self_comparison_policy(SelfComparisonPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_self_comparison_policy(SelfComparisonPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
