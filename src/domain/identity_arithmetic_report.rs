//! Common Lisp identity-arithmetic detection: an arithmetic form with a
//! redundant identity operand — `(+ x 0)`, `(* x 1)`, `(- x 0)`, `(/ x 1)`.
//! Adding `0`, multiplying by `1`, subtracting `0`, or dividing by `1` returns
//! the other operand unchanged, so the identity literal is pure noise, common
//! after mechanical macro expansion or when operands are edited away.
//!
//! Only the *integer* identity literals `0` and `1` are flagged — `0.0`/`1.0`
//! coerce the result to a float and are meaningful, so they are left alone. For
//! `+` and `*` (commutative) the identity may be in any operand position; for
//! `-` and `/` only a *non-first* operand is an identity (`(- 0 x)` negates and
//! `(/ 1 x)` is a reciprocal, so a leading `0`/`1` is not redundant).
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

/// For an arithmetic head, the identity literal that is redundant and the first
/// operand index at which it may appear (1 for the commutative `+`/`*`, 2 for
/// `-`/`/` whose leading operand is not an identity position).
fn identity_for(head: &str) -> Option<(&'static str, usize)> {
    match head {
        "+" => Some(("0", 1)),
        "*" => Some(("1", 1)),
        "-" => Some(("0", 2)),
        "/" => Some(("1", 2)),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct IdentityArithmeticItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The arithmetic operator (`+`, `-`, `*`, or `/`).
    pub operator: String,
    /// The redundant identity literal (`0` or `1`).
    pub identity: &'static str,
}

#[derive(Debug)]
pub struct IdentityArithmeticSummary {
    pub arithmetic_form_count: usize,
    pub violations: Vec<IdentityArithmeticItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct IdentityArithmeticPolicyOptions {
    fail_on_violation: bool,
}

impl IdentityArithmeticPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct IdentityArithmeticPolicy {
    pub fail_on_violation: bool,
    pub arithmetic_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

fn examine_form(
    view: &ExpressionView,
    path: &Path,
    arithmetic_form_count: &mut usize,
    violations: &mut Vec<IdentityArithmeticItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    // The operators are single ASCII glyphs; an exact match is enough.
    let Some((identity, first_index)) = identity_for(head) else {
        return;
    };
    *arithmetic_form_count += 1;

    // Report the first identity operand at an identity position; one form with
    // two redundant identities is still one form's redundancy.
    let has_identity = view
        .children
        .iter()
        .skip(first_index)
        .any(|child| atom_text(child) == Some(identity));
    if has_identity {
        violations.push(IdentityArithmeticItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            identity,
        });
    }
}

/// Collects every arithmetic form with a redundant identity operand across a
/// whole file, along with the total number of arithmetic forms scanned.
pub fn collect_identity_arithmetic(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<IdentityArithmeticItem>)> {
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

pub fn summarize_identity_arithmetic(
    arithmetic_form_count: usize,
    violations: Vec<IdentityArithmeticItem>,
) -> IdentityArithmeticSummary {
    IdentityArithmeticSummary {
        arithmetic_form_count,
        violations,
    }
}

pub fn evaluate_identity_arithmetic_policy(
    options: IdentityArithmeticPolicyOptions,
    summary: &IdentityArithmeticSummary,
) -> IdentityArithmeticPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    IdentityArithmeticPolicy {
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

    fn forms(input: &str) -> (usize, Vec<IdentityArithmeticItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_identity_arithmetic(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect identity arithmetic")
    }

    #[test]
    fn flags_add_zero() {
        let (count, violations) = forms("(+ x 0)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "+");
        assert_eq!(violations[0].identity, "0");
    }

    #[test]
    fn flags_add_zero_in_any_position() {
        let (_, violations) = forms("(+ 0 x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_multiply_one() {
        let (_, violations) = forms("(* x 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].identity, "1");
    }

    #[test]
    fn flags_subtract_zero() {
        let (_, violations) = forms("(- x 0)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn flags_divide_by_one() {
        let (_, violations) = forms("(/ x 1)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_leading_zero_in_subtraction() {
        // (- 0 x) is negation, not an identity.
        let (_, violations) = forms("(- 0 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_leading_one_in_division() {
        // (/ 1 x) is a reciprocal, not an identity.
        let (_, violations) = forms("(/ 1 x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_float_identities() {
        // (* x 1.0) coerces to a float; it is meaningful.
        let (_, violations) = forms("(* x 1.0)");
        assert!(violations.is_empty());
        let (_, violations) = forms("(+ x 0.0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_non_identity_literals() {
        let (_, violations) = forms("(+ x 2)");
        assert!(violations.is_empty());
        let (_, violations) = forms("(* x 0)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = forms("(max x 0)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_form_nested_in_a_body() {
        let (_, violations) = forms("(defun f (x) (list (+ x 0)))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(+ x 0)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_identity_arithmetic(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect identity arithmetic");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = forms("(+ x 0)");
        let summary = summarize_identity_arithmetic(count, items);

        let quiet = evaluate_identity_arithmetic_policy(
            IdentityArithmeticPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_identity_arithmetic_policy(
            IdentityArithmeticPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
