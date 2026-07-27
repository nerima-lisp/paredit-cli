//! Common Lisp one-step-arithmetic detection: a two-argument `+`/`-` where one
//! operand is the literal `1`, which the language provides a unary shorthand
//! for. `(1+ n)` is defined as `(+ n 1)` and `(1- n)` as `(- n 1)`, so the
//! rewrite is exact (same value, same numeric-type contract) and the shorthand
//! states the unit step directly.
//!
//! The operator's algebra decides which shapes qualify. `+` is commutative, so
//! `(+ x 1)` and `(+ 1 x)` both become `(1+ x)`. `-` is not: only `(- x 1)`
//! becomes `(1- x)`, since `(- 1 x)` is `1 - x`, which has no unary shorthand.
//!
//! Only the bare integer literal `1` is matched. A float `1.0` is left alone:
//! `(+ x 1.0)` can coerce `x` to a float, so it is not `(1+ x)`. A `#x1`/prefixed
//! spelling, a three-argument form, and a reader-conditional operand are all
//! left alone.
//!
//! The fix rewrites the form as `(1+ x)` / `(1- x)`, copying the non-`1` operand
//! from its exact source, so the rule is auto-fixable.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

/// Whether `view` is the bare integer `1` literal (no reader prefixes, so `#x1`
/// and a prefixed `,1` are excluded; `1.0` is a different spelling, excluded).
fn is_one_literal(view: &ExpressionView) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some("1")
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct OneStepArithmeticItem {
    pub path: PathBuf,
    /// The span of the whole `(+ x 1)` / `(- x 1)` form.
    pub span: ByteSpan,
    /// The span of the non-`1` operand `x`.
    pub operand_span: ByteSpan,
    /// The suggested shorthand operator (`1+`/`1-`).
    pub shorthand: &'static str,
}

#[derive(Debug)]
pub struct OneStepArithmeticSummary {
    pub arithmetic_form_count: usize,
    pub violations: Vec<OneStepArithmeticItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct OneStepArithmeticPolicyOptions {
    fail_on_violation: bool,
}

impl OneStepArithmeticPolicyOptions {
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
pub struct OneStepArithmeticPolicy {
    pub fail_on_violation: bool,
    pub arithmetic_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_form(
    view: &ExpressionView,
    path: &Path,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<OneStepArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let is_plus = head == "+";
    let is_minus = head == "-";
    if !is_plus && !is_minus {
        return;
    }
    *arithmetic_form_count += 1;

    // children[0] is the operator; require exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let left = &view.children[1];
    let right = &view.children[2];
    if is_reader_conditional(left) || is_reader_conditional(right) {
        return;
    }

    // `+` takes the `1` on either side; `-` only as the subtrahend `(- x 1)`.
    let (operand, shorthand) = if is_plus {
        if is_one_literal(right) {
            (left, "1+")
        } else if is_one_literal(left) {
            (right, "1+")
        } else {
            return;
        }
    } else if is_one_literal(right) {
        (left, "1-")
    } else {
        return;
    };

    violations.push(OneStepArithmeticItem {
        path: path.to_path_buf(),
        span: view.span,
        operand_span: operand.span,
        shorthand,
    });
}

/// Collects every two-argument `+`/`-` of a literal `1` across a whole file,
/// along with the total number of `+`/`-` forms scanned.
pub fn collect_one_step_arithmetic(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<OneStepArithmeticItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, path, &mut arithmetic_form_count, &mut violations);
        });
    }
    Ok((arithmetic_form_count, violations))
}

#[must_use]
pub const fn summarize_one_step_arithmetic(
    arithmetic_form_count: usize,
    violations: Vec<OneStepArithmeticItem>,
) -> OneStepArithmeticSummary {
    OneStepArithmeticSummary {
        arithmetic_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_one_step_arithmetic_policy(
    options: OneStepArithmeticPolicyOptions,
    summary: &OneStepArithmeticSummary,
) -> OneStepArithmeticPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    OneStepArithmeticPolicy {
        fail_on_violation: options.fail_on_violation(),
        arithmetic_form_count: summary.arithmetic_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(input: &str) -> (usize, Vec<OneStepArithmeticItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_one_step_arithmetic(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect one-step arithmetic")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn flags_plus_one_on_the_right() {
        let source = "(+ n 1)";
        let (count, violations) = forms(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].shorthand, "1+");
        assert_eq!(slice(source, violations[0].operand_span), "n");
    }

    #[test]
    fn flags_plus_one_on_the_left() {
        let source = "(+ 1 (length xs))";
        let (_, violations) = forms(source);
        assert_eq!(violations[0].shorthand, "1+");
        assert_eq!(slice(source, violations[0].operand_span), "(length xs)");
    }

    #[test]
    fn flags_minus_one_on_the_right() {
        let source = "(- count 1)";
        let (_, violations) = forms(source);
        assert_eq!(violations[0].shorthand, "1-");
        assert_eq!(slice(source, violations[0].operand_span), "count");
    }

    #[test]
    fn does_not_flag_one_minus_x() {
        // (- 1 x) is 1 - x; no unary shorthand.
        let (count, violations) = forms("(- 1 x)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_one() {
        assert!(forms("(+ x 1.0)").1.is_empty());
        assert!(forms("(- x 1.0)").1.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_one_literal() {
        let (count, violations) = forms("(+ x 2)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        let (_, violations) = forms("(+ x 1 y)");
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_form() {
        let (_, violations) = forms("(setf i (+ i 1))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(+ n 1)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_one_step_arithmetic(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect one-step arithmetic");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = forms("(+ n 1)");
        let summary = summarize_one_step_arithmetic(count, items);

        let quiet = evaluate_one_step_arithmetic_policy(
            OneStepArithmeticPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_one_step_arithmetic_policy(
            OneStepArithmeticPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
