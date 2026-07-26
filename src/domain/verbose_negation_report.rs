//! Common Lisp verbose-negation detection: negation written the long way, which
//! the unary `-` expresses directly — `(- 0 x)` (zero minus x), `(* x -1)` and
//! `(* -1 x)` (times minus one) are all exactly `(- x)`. The unary form states
//! "negate this" without a spurious constant operand.
//!
//! Each shape is an exact rewrite that evaluates `x` once:
//!
//!   - `(- 0 X)` → `(- X)`   (0 − X = −X; only the *leading* zero, so this does
//!     not overlap `identity-arithmetic`'s trailing `(- x 0)`).
//!   - `(* X -1)` / `(* -1 X)` → `(- X)`   (`*` commutes).
//!
//! Only the bare *integer* literals `0` and `-1` count. A float spelling like
//! `-1.0` would coerce the result type (`(* 2 -1.0)` is a float while `(- 2)` is
//! an integer), so it is left alone, as is a reader-conditional operand.
//!
//! The fix rewrites the form as `(- X)`, copying `X` from source, so the rule is
//! auto-fixable.
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

/// Whether `view` is the bare integer literal `literal` (no reader prefixes).
fn is_int_literal(view: &ExpressionView, literal: &str) -> bool {
    view.reader_prefixes.is_empty() && atom_text(view) == Some(literal)
}

/// A reader-conditional atom (`#+feature`/`#-feature`) is build-dependent, so a
/// form containing one has no settled operand list.
fn is_reader_conditional(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

#[derive(Debug, Clone)]
pub struct VerboseNegationItem {
    pub path: PathBuf,
    /// The span of the whole `(- 0 x)`/`(* x -1)` form.
    pub span: ByteSpan,
    /// The span of the operand `X` that is being negated.
    pub operand_span: ByteSpan,
}

#[derive(Debug)]
pub struct VerboseNegationSummary {
    pub arithmetic_form_count: usize,
    pub violations: Vec<VerboseNegationItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct VerboseNegationPolicyOptions {
    fail_on_violation: bool,
}

impl VerboseNegationPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct VerboseNegationPolicy {
    pub fail_on_violation: bool,
    pub arithmetic_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_form(
    view: &ExpressionView,
    path: &Path,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<VerboseNegationItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !matches!(head, "-" | "*") {
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

    // `(- 0 X)` negates X; only the leading zero (trailing is identity-arithmetic).
    // `(* X -1)` / `(* -1 X)` negate the other operand.
    let operand = if head == "-" {
        if is_int_literal(left, "0") {
            right
        } else {
            return;
        }
    } else if is_int_literal(left, "-1") {
        right
    } else if is_int_literal(right, "-1") {
        left
    } else {
        return;
    };

    violations.push(VerboseNegationItem {
        path: path.to_path_buf(),
        span: view.span,
        operand_span: operand.span,
    });
}

/// Collects every verbose negation across a whole file, along with the total
/// number of `-`/`*` forms scanned.
pub fn collect_verbose_negations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<VerboseNegationItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut arithmetic_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(subview, path, &mut arithmetic_form_count, &mut violations)
        });
    }
    Ok((arithmetic_form_count, violations))
}

pub fn summarize_verbose_negations(
    arithmetic_form_count: usize,
    violations: Vec<VerboseNegationItem>,
) -> VerboseNegationSummary {
    VerboseNegationSummary {
        arithmetic_form_count,
        violations,
    }
}

pub fn evaluate_verbose_negation_policy(
    options: VerboseNegationPolicyOptions,
    summary: &VerboseNegationSummary,
) -> VerboseNegationPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    VerboseNegationPolicy {
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

    fn negations(input: &str) -> (usize, Vec<VerboseNegationItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_verbose_negations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect verbose negations")
    }

    fn slice(source: &str, span: ByteSpan) -> &str {
        &source[span.start().get()..span.end().get()]
    }

    #[test]
    fn zero_minus_x_negates() {
        let source = "(- 0 count)";
        let (count, violations) = negations(source);
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(slice(source, violations[0].operand_span), "count");
    }

    #[test]
    fn times_minus_one_negates() {
        let source = "(* delta -1)";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "delta");
    }

    #[test]
    fn minus_one_times_negates_commuted() {
        let source = "(* -1 delta)";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "delta");
    }

    #[test]
    fn preserves_compound_operand_source() {
        let source = "(- 0 (compute x))";
        let (_, violations) = negations(source);
        assert_eq!(slice(source, violations[0].operand_span), "(compute x)");
    }

    #[test]
    fn does_not_flag_trailing_zero() {
        // (- x 0) is identity-arithmetic's job, not negation.
        let (_, violations) = negations("(- x 0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_minus_one() {
        // (* x -1.0) would coerce the result type.
        let (_, violations) = negations("(* x -1.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_constants() {
        assert!(negations("(- 5 x)").1.is_empty());
        assert!(negations("(* x -2)").1.is_empty());
    }

    #[test]
    fn does_not_flag_three_operands() {
        assert!(negations("(- 0 x y)").1.is_empty());
        assert!(negations("(* -1 x y)").1.is_empty());
    }

    #[test]
    fn finds_a_nested_negation() {
        let (_, violations) = negations("(setf d (- 0 delta))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(- 0 x)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_verbose_negations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect verbose negations");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = negations("(- 0 x)");
        let summary = summarize_verbose_negations(count, items);

        let quiet =
            evaluate_verbose_negation_policy(VerboseNegationPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_verbose_negation_policy(VerboseNegationPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
