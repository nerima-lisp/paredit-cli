//! Common Lisp single-argument-comparison detection: a numeric comparison
//! (`<`, `>`, `<=`, `>=`, `=`, `/=`) called with exactly one argument. These
//! operators are variadic and defined to return `t` for a single argument, so
//! `(< x)`, `(= x)`, `(/= x)` and friends are *vacuously true* no matter what
//! `x` is — the comparison does nothing. In practice this is a missing operand
//! (`(< x y)` mistyped as `(< x)`), which no compiler flags because the call is
//! technically legal.
//!
//! Only these six variadic numeric comparisons are considered. The fixed-arity
//! equality tests (`eq`/`eql`/`equal`/`equalp`) are handled by the
//! `equality-arity` rule instead, since for them a single argument is an arity
//! error rather than a vacuous truth. A lone reader conditional (`#+`/`#-`) as
//! the sole argument is exempt: it may expand to zero or more arguments.
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

/// The variadic numeric comparison operators for which a single argument is
/// vacuously true. Matched exactly (these symbols have no alphabetic case).
const COMPARISONS: [&str; 6] = ["<", ">", "<=", ">=", "=", "/="];

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom does not represent one argument.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleArgComparisonItem {
    pub path: PathBuf,
    /// The span of the whole `(< x)`-style form.
    pub span: ByteSpan,
    /// The comparison operator (`<`, `>`, `<=`, `>=`, `=`, or `/=`).
    pub operator: &'static str,
}

#[derive(Debug)]
pub struct SingleArgComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<SingleArgComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleArgComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl SingleArgComparisonPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct SingleArgComparisonPolicy {
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
    violations: &mut Vec<SingleArgComparisonItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = COMPARISONS.iter().copied().find(|op| *op == head) else {
        return;
    };
    *comparison_form_count += 1;

    // children[0] is the operator; a single argument means exactly two children.
    if view.children.len() != 2 || is_reader_conditional(&view.children[1]) {
        return;
    }
    violations.push(SingleArgComparisonItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
    });
}

/// Collects every single-argument numeric comparison across a whole file, along
/// with the total number of comparison forms scanned.
pub fn collect_single_arg_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SingleArgComparisonItem>)> {
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

pub fn summarize_single_arg_comparisons(
    comparison_form_count: usize,
    violations: Vec<SingleArgComparisonItem>,
) -> SingleArgComparisonSummary {
    SingleArgComparisonSummary {
        comparison_form_count,
        violations,
    }
}

pub fn evaluate_single_arg_comparison_policy(
    options: SingleArgComparisonPolicyOptions,
    summary: &SingleArgComparisonSummary,
) -> SingleArgComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleArgComparisonPolicy {
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

    fn comparisons(input: &str) -> (usize, Vec<SingleArgComparisonItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_arg_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-arg comparisons")
    }

    #[test]
    fn flags_single_arg_less_than() {
        let (count, violations) = comparisons("(when (< x) (go))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "<");
    }

    #[test]
    fn flags_every_comparison_operator() {
        for op in ["<", ">", "<=", ">=", "=", "/="] {
            let (_, violations) = comparisons(&format!("({op} x)"));
            assert_eq!(violations.len(), 1, "operator {op} should be flagged");
            assert_eq!(violations[0].operator, op);
        }
    }

    #[test]
    fn flags_a_single_complex_argument() {
        let (_, violations) = comparisons("(= (length xs))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "=");
    }

    #[test]
    fn does_not_flag_two_arguments() {
        let (count, violations) = comparisons("(< x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_many_arguments() {
        let (_, violations) = comparisons("(<= a b c)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_equality_predicates() {
        // eq/eql/equal are the equality-arity rule's territory, not this one.
        let (count, violations) = comparisons("(eql x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = comparisons("(< #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_comparison_heads() {
        let (count, violations) = comparisons("(max x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_arg_comparison() {
        let (_, violations) = comparisons("(and ready (> total))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, ">");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(< x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_single_arg_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-arg comparisons");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = comparisons("(< x)");
        let summary = summarize_single_arg_comparisons(count, items);

        let quiet = evaluate_single_arg_comparison_policy(
            SingleArgComparisonPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_single_arg_comparison_policy(
            SingleArgComparisonPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
