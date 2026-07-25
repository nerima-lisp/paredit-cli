//! Common Lisp `eq`-on-a-number detection: a call to `eq` with a numeric
//! literal argument, such as `(eq n 5)` or `(eq (len x) 0)`. `eq` tests
//! object identity, and the standard only guarantees identity for the same
//! object — numbers are not required to be `eq` even when mathematically
//! equal. Small fixnums often happen to be `eq` on a given implementation,
//! but bignums and floats are not, so `(eq n 5)` silently works until `n`
//! grows past the fixnum range. The correct comparison is `eql` (or `=` for
//! numeric equality across types).
//!
//! Only `eq` is covered: `eql` and `=` compare numbers correctly, so they
//! are not flagged. A number literal is recognized by Rust's own integer or
//! float parse, which rejects the classic gotcha symbols `1+` and `1-`
//! (increment/decrement functions, not numbers); the first character must
//! also be a digit, sign, or dot, so the symbols `inf`/`nan` — which Rust's
//! float parser would otherwise accept — are excluded.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`], since such a call can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, list_head};

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

fn number_argument(view: &ExpressionView) -> Option<&str> {
    atom_text(view).filter(|text| is_number_literal(text))
}

#[derive(Debug, Clone)]
pub struct EqNumberComparisonItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The span of the `eq` head symbol, for an `eq` -> `eql` fix.
    pub head_span: ByteSpan,
    pub literal: String,
}

#[derive(Debug)]
pub struct EqNumberComparisonSummary {
    pub comparison_form_count: usize,
    pub violations: Vec<EqNumberComparisonItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct EqNumberComparisonPolicyOptions {
    fail_on_violation: bool,
}

impl EqNumberComparisonPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct EqNumberComparisonPolicy {
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
    violations: &mut Vec<EqNumberComparisonItem>,
) {
    if !list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("eq")) {
        return;
    }
    *comparison_form_count += 1;

    // Report the first numeric-literal argument (after the operator); a call
    // with two number literals is still one bug, not two.
    if let Some(literal) = view.children.iter().skip(1).find_map(number_argument) {
        violations.push(EqNumberComparisonItem {
            path: path.to_path_buf(),
            span: view.span,
            head_span: view.children[0].span,
            literal: literal.to_owned(),
        });
    }
}

/// Collects every `eq` call with a numeric-literal argument across a whole
/// file, along with the total number of `eq` calls scanned.
pub fn collect_eq_number_comparisons(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<EqNumberComparisonItem>)> {
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

pub fn summarize_eq_number_comparisons(
    comparison_form_count: usize,
    violations: Vec<EqNumberComparisonItem>,
) -> EqNumberComparisonSummary {
    EqNumberComparisonSummary {
        comparison_form_count,
        violations,
    }
}

pub fn evaluate_eq_number_comparison_policy(
    options: EqNumberComparisonPolicyOptions,
    summary: &EqNumberComparisonSummary,
) -> EqNumberComparisonPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    EqNumberComparisonPolicy {
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

    fn comparisons(input: &str) -> (usize, Vec<EqNumberComparisonItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_eq_number_comparisons(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect eq number comparisons")
    }

    #[test]
    fn flags_eq_against_an_integer_literal() {
        let (comparison_form_count, violations) = comparisons("(eq n 5)");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "5");
    }

    #[test]
    fn flags_eq_against_a_float_literal() {
        let (_, violations) = comparisons("(eq (ratio x) 1.0)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].literal, "1.0");
    }

    #[test]
    fn flags_eq_against_a_negative_literal() {
        let (_, violations) = comparisons("(eq delta -1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_eql_or_numeric_equality() {
        let (comparison_form_count, violations) = comparisons("(and (eql n 5) (= n 5))");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_eq_between_two_symbols() {
        let (comparison_form_count, violations) = comparisons("(eq x y)");
        assert_eq!(comparison_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_increment_function_symbol() {
        // `1+` is a symbol (the increment function), not a number literal.
        let (_, violations) = comparisons("(eq op '1+)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_comparison_nested_in_a_function_body() {
        let (comparison_form_count, violations) =
            comparisons("(defun f (n) (when (eq n 0) :zero))");
        assert_eq!(comparison_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(eq n 5)").expect("parse input");
        let (comparison_form_count, violations) =
            collect_eq_number_comparisons(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect eq number comparisons");
        assert_eq!(comparison_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (comparison_form_count, items) = comparisons("(eq n 5)");
        let summary = summarize_eq_number_comparisons(comparison_form_count, items);

        let quiet = evaluate_eq_number_comparison_policy(
            EqNumberComparisonPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_eq_number_comparison_policy(
            EqNumberComparisonPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
