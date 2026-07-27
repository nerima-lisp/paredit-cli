//! Common Lisp single-operand `+`/`*` detection: `(+ X)` or `(* X)` with
//! exactly one operand. A one-argument `+` or `*` folds the additive/multiplicative
//! identity over a single value and returns that value verbatim — `(+ X)` is `X`
//! and `(* X)` is `X`. The wrapper is pure redundancy, common after mechanical
//! macro expansion or when a `reduce`/`apply` was inlined down to one argument.
//!
//! Only `+` and `*` are flagged, and only in their single-operand shape:
//!
//!   - `(- X)` is *negation* and `(/ X)` is *reciprocal* — meaningful unary ops,
//!     never flagged.
//!   - The zero-operand identities `(+)` (which is `0`) and `(*)` (which is `1`)
//!     are legitimate macro-expansion building blocks and are left alone.
//!   - Two-or-more-operand forms are meaningful arithmetic.
//!   - A lone reader conditional (`#+`/`#-`) as the sole operand is exempt: it
//!     may expand to zero or one operand depending on the build.
//!
//! Because the fix simply unwraps to the sole operand (an already-present
//! subexpression, copied verbatim), this rule is auto-fixable — the same fix
//! shape as [`crate::domain::single_operand_boolean_report`]. Contrast
//! [`crate::domain::identity_arithmetic_report`], which would have to *delete*
//! an operand from a multi-argument form and is therefore report-only.
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

/// The canonical operator name for a `+`/`*` head, or `None` otherwise. Unlike
/// the boolean operators these are punctuation, so an exact match is used (no
/// case folding — `+` has no alphabetic case).
fn identity_fold_operator(head: &str) -> Option<&'static str> {
    match head {
        "+" => Some("+"),
        "*" => Some("*"),
        _ => None,
    }
}

/// A reader-conditional atom (`#+feature`/`#-feature`) reads together with the
/// form that follows it, so a single such atom operand does not represent one
/// evaluated operand. Mirrors the guard used by the other progn/boolean lints.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct SingleOperandArithmeticItem {
    pub path: PathBuf,
    /// The span of the whole `(+ X)`/`(* X)` form.
    pub span: ByteSpan,
    /// The operator (`+` or `*`).
    pub operator: &'static str,
    /// The span of the sole operand `X` (lets a fix substitute its source).
    pub inner_span: ByteSpan,
}

#[derive(Debug)]
pub struct SingleOperandArithmeticSummary {
    pub arithmetic_form_count: usize,
    pub violations: Vec<SingleOperandArithmeticItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SingleOperandArithmeticPolicyOptions {
    fail_on_violation: bool,
}

impl SingleOperandArithmeticPolicyOptions {
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
pub struct SingleOperandArithmeticPolicy {
    pub fail_on_violation: bool,
    pub arithmetic_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_arithmetic(
    view: &ExpressionView,
    path: &Path,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<SingleOperandArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(operator) = identity_fold_operator(head) else {
        return;
    };
    *arithmetic_form_count += 1;

    // children[0] is the operator; a single operand means exactly two children.
    if view.children.len() != 2 {
        return;
    }
    let operand = &view.children[1];
    if is_reader_conditional(operand) {
        return;
    }
    violations.push(SingleOperandArithmeticItem {
        path: path.to_path_buf(),
        span: view.span,
        operator,
        inner_span: operand.span,
    });
}

/// Collects every single-operand `+`/`*` across a whole file, along with the
/// total number of `+`/`*` forms scanned.
pub fn collect_single_operand_arithmetic(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<SingleOperandArithmeticItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_arithmetic(subview, path, &mut arithmetic_form_count, &mut violations);
        });
    }
    Ok((arithmetic_form_count, violations))
}

#[must_use]
pub const fn summarize_single_operand_arithmetic(
    arithmetic_form_count: usize,
    violations: Vec<SingleOperandArithmeticItem>,
) -> SingleOperandArithmeticSummary {
    SingleOperandArithmeticSummary {
        arithmetic_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_single_operand_arithmetic_policy(
    options: SingleOperandArithmeticPolicyOptions,
    summary: &SingleOperandArithmeticSummary,
) -> SingleOperandArithmeticPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SingleOperandArithmeticPolicy {
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

    fn arithmetic(input: &str) -> (usize, Vec<SingleOperandArithmeticItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_single_operand_arithmetic(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect single-operand arithmetic")
    }

    #[test]
    fn flags_single_operand_plus() {
        let (count, violations) = arithmetic("(+ x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "+");
    }

    #[test]
    fn flags_single_operand_star() {
        let (_, violations) = arithmetic("(* (compute))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "*");
    }

    #[test]
    fn inner_span_covers_only_the_operand() {
        let (_, violations) = arithmetic("(+ (foo bar))");
        let inner = violations[0].inner_span;
        assert!(inner.start().get() > violations[0].span.start().get());
        assert!(inner.end().get() < violations[0].span.end().get());
    }

    #[test]
    fn does_not_flag_two_operands() {
        let (count, violations) = arithmetic("(+ x y)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_the_empty_identity() {
        let (_, violations) = arithmetic("(+)");
        assert!(violations.is_empty());
        let (_, violations) = arithmetic("(*)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_unary_minus_or_divide() {
        // (- x) negates and (/ x) is a reciprocal — both meaningful.
        let (count, violations) = arithmetic("(- x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
        let (count, violations) = arithmetic("(/ x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_lone_reader_conditional() {
        let (_, violations) = arithmetic("(+ #+sbcl x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = arithmetic("(list x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_nested_single_operand_arithmetic() {
        // Outer (+ ...) has two operands; the inner (* z) is single-operand.
        let (_, violations) = arithmetic("(+ y (* z))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "*");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(+ x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_single_operand_arithmetic(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect single-operand arithmetic");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = arithmetic("(+ x)");
        let summary = summarize_single_operand_arithmetic(count, items);

        let quiet = evaluate_single_operand_arithmetic_policy(
            SingleOperandArithmeticPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_single_operand_arithmetic_policy(
            SingleOperandArithmeticPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
