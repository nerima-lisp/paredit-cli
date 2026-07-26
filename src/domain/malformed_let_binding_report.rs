//! Common Lisp malformed-`let`-binding detection: a `let` or `let*` binding
//! that is neither a bare symbol nor a `(var)` / `(var value)` list. A binding
//! list element with zero elements (`()`) or three or more elements
//! (`(x 1 2)`) is a program error — `let`/`let*` bindings, unlike `do`/`do*`
//! step bindings, never take a third form.
//!
//! The highest-value catch is a dropped-parenthesis typo: writing
//! `(let ((x 1 y 2)) …)` when `(let ((x 1) (y 2)) …)` was meant produces one
//! four-element binding, which this rule flags directly.
//!
//! Scoped to `let`/`let*` on purpose: `do`/`do*` bindings legitimately carry a
//! third `step` form, so they are never inspected here. Bare-symbol bindings
//! (`(let (x y) …)`) and single-element `(var)` bindings are valid and left
//! alone. This report walks the whole tree via the shared
//! [`crate::domain::view_query::for_each_subview`], since a `let` can appear
//! anywhere in a body.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::expression_equality::render_expression;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{for_each_subview, is_paren_list, list_head};

const LET_HEADS: [&str; 2] = ["let", "let*"];

#[derive(Debug, Clone)]
pub struct MalformedLetBindingItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub binding: String,
    pub element_count: usize,
}

#[derive(Debug)]
pub struct MalformedLetBindingSummary {
    pub let_form_count: usize,
    pub violations: Vec<MalformedLetBindingItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct MalformedLetBindingPolicyOptions {
    fail_on_violation: bool,
}

impl MalformedLetBindingPolicyOptions {
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct MalformedLetBindingPolicy {
    pub fail_on_violation: bool,
    pub let_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_let(
    view: &ExpressionView,
    path: &Path,
    let_form_count: &mut usize,
    violations: &mut Vec<MalformedLetBindingItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !LET_HEADS.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        return;
    }
    let Some(binding_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(binding_list) {
        return;
    }
    *let_form_count += 1;

    for binding in &binding_list.children {
        // A bare-symbol binding is valid; only list-shaped bindings can carry
        // the wrong number of elements.
        if !is_paren_list(binding) {
            continue;
        }
        let element_count = binding.children.len();
        if element_count == 0 || element_count > 2 {
            violations.push(MalformedLetBindingItem {
                path: path.to_path_buf(),
                span: binding.span,
                binding: render_expression(binding),
                element_count,
            });
        }
    }
}

/// Collects every malformed `let`/`let*` binding across a whole file, along
/// with the total number of `let`/`let*` forms scanned.
pub fn collect_malformed_let_bindings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<MalformedLetBindingItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut let_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_let(subview, path, &mut let_form_count, &mut violations);
        });
    }
    Ok((let_form_count, violations))
}

pub fn summarize_malformed_let_bindings(
    let_form_count: usize,
    violations: Vec<MalformedLetBindingItem>,
) -> MalformedLetBindingSummary {
    MalformedLetBindingSummary {
        let_form_count,
        violations,
    }
}

pub fn evaluate_malformed_let_binding_policy(
    options: MalformedLetBindingPolicyOptions,
    summary: &MalformedLetBindingSummary,
) -> MalformedLetBindingPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    MalformedLetBindingPolicy {
        fail_on_violation: options.fail_on_violation(),
        let_form_count: summary.let_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(input: &str) -> (usize, Vec<MalformedLetBindingItem>) {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_malformed_let_bindings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect malformed let bindings")
    }

    #[test]
    fn flags_a_three_element_binding() {
        let (let_form_count, violations) = bindings("(let ((x 1 2)) x)");
        assert_eq!(let_form_count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 3);
    }

    #[test]
    fn flags_a_dropped_paren_binding() {
        let (_, violations) = bindings("(let ((x 1 y 2)) x)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 4);
    }

    #[test]
    fn flags_an_empty_binding() {
        let (_, violations) = bindings("(let (()) 1)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].element_count, 0);
    }

    #[test]
    fn does_not_flag_a_var_value_binding() {
        let (let_form_count, violations) = bindings("(let ((x 1) (y 2)) (+ x y))");
        assert_eq!(let_form_count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_single_element_binding() {
        let (_, violations) = bindings("(let ((x)) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_bare_symbol_binding() {
        let (_, violations) = bindings("(let (x y) (list x y))");
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_a_malformed_let_star_binding() {
        let (_, violations) = bindings("(let* ((x 1 2)) x)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_do_step_bindings() {
        let (let_form_count, violations) =
            bindings("(do ((i 0 (1+ i)) (n 10)) ((>= i n)) (print i))");
        assert_eq!(let_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn finds_a_let_nested_in_a_function_body() {
        let (let_form_count, violations) = bindings("(defun f () (let ((x 1 2)) x))");
        assert_eq!(let_form_count, 1);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(let ((x 1 2)) x)").expect("parse input");
        let (let_form_count, violations) =
            collect_malformed_let_bindings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect malformed let bindings");
        assert_eq!(let_form_count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (let_form_count, items) = bindings("(let ((x 1 2)) x)");
        let summary = summarize_malformed_let_bindings(let_form_count, items);

        let quiet = evaluate_malformed_let_binding_policy(
            MalformedLetBindingPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_malformed_let_binding_policy(
            MalformedLetBindingPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
