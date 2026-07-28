//! Common Lisp `setf`-arity detection: a `setq`, `psetq`, `setf`, or `psetf`
//! form with an odd number of arguments. All four take a flat sequence of
//! place/value *pairs*, so an odd argument count means the final place has no
//! value form — `(setf a 1 b)` and `(setq x 1 y)` are always errors, caught
//! only at macroexpansion (or silently miscompiled) rather than by the
//! reader. There is no valid odd-arity `setf`, so this report has no
//! false positives.
//!
//! An empty form (`(setf)`) has zero arguments — even — and is left alone;
//! this reports only a *non-zero* odd count.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`], since an assignment can
//! appear anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, list_head};

const ASSIGNMENT_HEADS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

#[derive(Debug, Clone)]
pub struct SetfArityItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub argument_count: usize,
}

#[derive(Debug)]
pub struct SetfAritySummary {
    pub assignment_form_count: usize,
    pub violations: Vec<SetfArityItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SetfArityPolicyOptions {
    fail_on_violation: bool,
}

impl SetfArityPolicyOptions {
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
pub struct SetfArityPolicy {
    pub fail_on_violation: bool,
    pub assignment_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_assignment(
    view: &ExpressionView,
    path: &Path,
    assignment_form_count: &mut usize,
    violations: &mut Vec<SetfArityItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !ASSIGNMENT_HEADS
        .iter()
        .any(|candidate| head.eq_ignore_ascii_case(candidate))
    {
        return;
    }
    *assignment_form_count += 1;

    let argument_count = view.children.len() - 1;
    if argument_count > 0 && argument_count % 2 == 1 {
        violations.push(SetfArityItem {
            path: path.to_path_buf(),
            span: view.span,
            operator: head.to_owned(),
            argument_count,
        });
    }
}

/// Collects every odd-arity `setq`/`setf`/`psetq`/`psetf` across a whole
/// file, along with the total number of assignment forms scanned.
pub fn collect_setf_arity_violations(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<SetfArityItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_assignment(subview, path, &mut assignment_form_count, &mut violations);
        });
    }
    Ok((assignment_form_count, violations))
}

#[must_use]
pub const fn summarize_setf_arity_violations(
    assignment_form_count: usize,
    violations: Vec<SetfArityItem>,
) -> SetfAritySummary {
    SetfAritySummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_setf_arity_policy(
    options: SetfArityPolicyOptions,
    summary: &SetfAritySummary,
) -> SetfArityPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SetfArityPolicy {
        fail_on_violation: options.fail_on_violation(),
        assignment_form_count: summary.assignment_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<SetfArityItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_setf_arity_violations(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect setf arity violations")
    }

    #[test]
    fn flags_an_odd_arity_setf() {
        let (assignment_form_count, violations) = violations("(setf a 1 b)");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "setf");
        assert_eq!(violations[0].argument_count, 3);
    }

    #[test]
    fn flags_an_odd_arity_setq() {
        let (_, violations) = violations("(setq x 1 y)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "setq");
    }

    #[test]
    fn does_not_flag_a_well_formed_setf() {
        let (assignment_form_count, violations) = violations("(setf a 1 b 2)");
        assert_eq!(assignment_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_pair() {
        let (_, violations) = violations("(setf place value)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_setf() {
        let (assignment_form_count, violations) = violations("(setf)");
        assert_eq!(assignment_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_an_assignment_nested_in_a_function_body() {
        let (assignment_form_count, violations) = violations("(defun f () (psetq a 1 b))");
        assert_eq!(assignment_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(setf a 1 b)").expect("parse input");
        let (assignment_form_count, violations) =
            collect_setf_arity_violations(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect setf arity violations");
        assert_eq!(assignment_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (assignment_form_count, items) = violations("(setf a 1 b)");
        let summary = summarize_setf_arity_violations(assignment_form_count, items);

        let quiet = evaluate_setf_arity_policy(SetfArityPolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_setf_arity_policy(SetfArityPolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
