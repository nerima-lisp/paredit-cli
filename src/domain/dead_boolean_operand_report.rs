//! Common Lisp dead-boolean-operand detection: an `and` whose non-final
//! operand is the literal `nil`, or an `or` whose non-final operand is the
//! literal `t`. Both operators short-circuit — `and` stops at the first
//! `nil`, `or` at the first non-`nil` — so every operand after that constant
//! can never be evaluated. `(and a nil b)` is just `(progn a nil)` with `b`
//! dead; `(or a t b)` is `(progn a t)` with `b` dead. Almost always the
//! constant is a leftover or a typo for a real test.
//!
//! Only a *non-final* constant is flagged, because a trailing constant is a
//! legitimate default: `(or x y t)` returns `t` when `x` and `y` are `nil`,
//! and `(and x y nil)` deliberately yields `nil`. The complementary
//! constants are pure redundancy rather than dead code — a non-final `t` in
//! `and` or `nil` in `or` contributes nothing but does not shadow later
//! operands — so this report leaves them to the redundancy-focused lints.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`], since an `and`/`or` can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only. The empty list `()` is treated as the `nil`
//! literal it reads as.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_text, for_each_subview, is_paren_list, list_head};

fn is_nil_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("nil"))
        || (is_paren_list(view) && view.children.is_empty())
}

fn is_t_literal(view: &ExpressionView) -> bool {
    atom_text(view).is_some_and(|text| text.eq_ignore_ascii_case("t"))
}

#[derive(Debug, Clone)]
pub struct DeadBooleanOperandItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub constant: String,
}

#[derive(Debug)]
pub struct DeadBooleanOperandSummary {
    pub boolean_form_count: usize,
    pub violations: Vec<DeadBooleanOperandItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct DeadBooleanOperandPolicyOptions {
    fail_on_violation: bool,
}

impl DeadBooleanOperandPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct DeadBooleanOperandPolicy {
    pub fail_on_violation: bool,
    pub boolean_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_boolean(
    view: &ExpressionView,
    path: &Path,
    boolean_form_count: &mut usize,
    violations: &mut Vec<DeadBooleanOperandItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    // The constant whose appearance short-circuits this operator: `nil` for
    // `and`, `t` for `or`.
    let (constant, is_short_circuit): (&str, fn(&ExpressionView) -> bool) =
        if head.eq_ignore_ascii_case("and") {
            ("nil", is_nil_literal)
        } else if head.eq_ignore_ascii_case("or") {
            ("t", is_t_literal)
        } else {
            return;
        };
    *boolean_form_count += 1;

    let operands = &view.children[1..];
    // A non-final operand is any but the last; if the short-circuiting
    // constant appears there, later operands are unreachable.
    let last_index = operands.len().saturating_sub(1);
    if operands.iter().take(last_index).any(is_short_circuit) {
        violations.push(DeadBooleanOperandItem {
            path: path.to_path_buf(),
            span: view.span,
            head: head.to_owned(),
            constant: constant.to_owned(),
        });
    }
}

/// Collects every `and`/`or` with a short-circuiting non-final constant
/// across a whole file, along with the total number of `and`/`or` forms
/// scanned.
pub fn collect_dead_boolean_operands(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<DeadBooleanOperandItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut boolean_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_boolean(subview, path, &mut boolean_form_count, &mut violations);
        });
    }
    Ok((boolean_form_count, violations))
}

pub fn summarize_dead_boolean_operands(
    boolean_form_count: usize,
    violations: Vec<DeadBooleanOperandItem>,
) -> DeadBooleanOperandSummary {
    DeadBooleanOperandSummary {
        boolean_form_count,
        violations,
    }
}

pub fn evaluate_dead_boolean_operand_policy(
    options: DeadBooleanOperandPolicyOptions,
    summary: &DeadBooleanOperandSummary,
) -> DeadBooleanOperandPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    DeadBooleanOperandPolicy {
        fail_on_violation: options.fail_on_violation(),
        boolean_form_count: summary.boolean_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<DeadBooleanOperandItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_dead_boolean_operands(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect dead boolean operands")
    }

    #[test]
    fn flags_a_non_final_nil_in_and() {
        let (boolean_form_count, violations) = violations("(and a nil b)");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "and");
        assert_eq!(violations[0].constant, "nil");
    }

    #[test]
    fn flags_a_non_final_t_in_or() {
        let (_, violations) = violations("(or a t b)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "or");
        assert_eq!(violations[0].constant, "t");
    }

    #[test]
    fn does_not_flag_a_trailing_t_in_or_default_idiom() {
        let (boolean_form_count, violations) = violations("(or x y t)");
        assert_eq!(boolean_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_trailing_nil_in_and() {
        let (_, violations) = violations("(and x y nil)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_non_final_t_in_and_which_is_only_redundant() {
        let (_, violations) = violations("(and a t b)");
        assert!(violations.is_empty());
    }

    #[test]
    fn treats_empty_list_as_nil_in_and() {
        let (_, violations) = violations("(and a () b)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn finds_a_boolean_nested_in_a_function_body() {
        let (boolean_form_count, violations) = violations("(defun f (a b) (when (and a nil b) 1))");
        assert_eq!(boolean_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(and a nil b)").expect("parse input");
        let (boolean_form_count, violations) =
            collect_dead_boolean_operands(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect dead boolean operands");
        assert_eq!(boolean_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (boolean_form_count, items) = violations("(and a nil b)");
        let summary = summarize_dead_boolean_operands(boolean_form_count, items);

        let quiet = evaluate_dead_boolean_operand_policy(
            DeadBooleanOperandPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_dead_boolean_operand_policy(
            DeadBooleanOperandPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
