//! Common Lisp redundant-body-`progn` detection: a multi-form `(progn …)` that
//! appears as a body form of a macro whose body is *already* an implicit progn
//! — `when`, `unless`, `dolist`, `dotimes`, `block`, `lambda`, `let`, `let*`,
//! `flet`, `labels`, `macrolet`, `defun`, `defmacro`. In all of these, the forms
//! after the fixed prefix are evaluated in sequence, so wrapping them in an
//! explicit progn changes nothing: `(when c (progn a b))` is exactly
//! `(when c a b)`. The wrapper is habit carried over from languages without an
//! implicit progn.
//!
//! This is the implicit-body companion to two neighbouring rules:
//! [`crate::redundant_progn::domain`] owns the 0-form and 1-form progns
//! (redundant on their own, in any position), and
//! [`crate::nested_progn::domain`] owns a multi-form progn nested
//! directly inside another `progn`. This rule owns a multi-form progn in the
//! body of the *other* implicit-progn forms. The three never flag the same
//! span.
//!
//! Only slots at or after each form's body start are inspected — the binding
//! list of a `let`, the lambda list of a `defun`, the test of a `when`, etc. are
//! never body positions — so a `(progn …)` used as a binding init or a single
//! value expression is correctly left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head};

/// The index at which a form's implicit-progn body begins, or `None` if the head
/// is not one of the recognized implicit-progn forms. Everything from this index
/// onward is a body form (declarations and docstrings included, which are never
/// progns), so a `(progn …)` there is spliceable.
fn body_start(head: &str) -> Option<usize> {
    let after_two = [
        "when", "unless", "dolist", "dotimes", "block", "lambda", "let", "let*", "flet", "labels",
        "macrolet", "catch",
    ];
    let after_three = ["defun", "defmacro"];
    if after_two.iter().any(|name| head.eq_ignore_ascii_case(name)) {
        Some(2)
    } else if after_three
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        Some(3)
    } else {
        None
    }
}

/// Whether `view` is a `(progn …)` form.
fn is_progn(view: &ExpressionView) -> bool {
    is_paren_list(view) && list_head(view).is_some_and(|head| head.eq_ignore_ascii_case("progn"))
}

#[derive(Debug, Clone)]
pub struct RedundantBodyPrognItem {
    pub path: PathBuf,
    /// The span of the inner (redundant) progn.
    pub span: ByteSpan,
    /// The span covering just the inner progn's body forms, so a fix can splice
    /// that source in place of the wrapper.
    pub body_span: ByteSpan,
    /// The inner progn's body form count (always >= 2 here).
    pub body_form_count: usize,
    /// The enclosing form's head (`when`, `let`, `defun`, …), for the message.
    pub parent: String,
}

#[derive(Debug)]
pub struct RedundantBodyPrognSummary {
    pub implicit_progn_form_count: usize,
    pub violations: Vec<RedundantBodyPrognItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct RedundantBodyPrognPolicyOptions {
    fail_on_violation: bool,
}

impl RedundantBodyPrognPolicyOptions {
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
pub struct RedundantBodyPrognPolicy {
    pub fail_on_violation: bool,
    pub implicit_progn_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_form(
    view: &ExpressionView,
    path: &Path,
    implicit_progn_form_count: &mut usize,
    violations: &mut Vec<RedundantBodyPrognItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(start) = body_start(head) else {
        return;
    };
    *implicit_progn_form_count += 1;

    // Any body form that is itself a multi-form progn splices redundantly.
    for child in view.children.iter().skip(start) {
        if !is_progn(child) {
            continue;
        }
        let body = &child.children[1..];
        if body.len() >= 2 {
            let body_span = ByteSpan::new(body[0].span.start(), body[body.len() - 1].span.end());
            violations.push(RedundantBodyPrognItem {
                path: path.to_path_buf(),
                span: child.span,
                body_span,
                body_form_count: body.len(),
                parent: head.to_ascii_lowercase(),
            });
        }
    }
}

/// Collects every multi-form progn used as a body form of an implicit-progn
/// macro across a whole file, along with the total number of such macro forms
/// scanned.
pub fn collect_redundant_body_progns(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<RedundantBodyPrognItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut implicit_progn_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_form(
                subview,
                path,
                &mut implicit_progn_form_count,
                &mut violations,
            );
        });
    }
    Ok((implicit_progn_form_count, violations))
}

#[must_use]
pub const fn summarize_redundant_body_progns(
    implicit_progn_form_count: usize,
    violations: Vec<RedundantBodyPrognItem>,
) -> RedundantBodyPrognSummary {
    RedundantBodyPrognSummary {
        implicit_progn_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_redundant_body_progn_policy(
    options: RedundantBodyPrognPolicyOptions,
    summary: &RedundantBodyPrognSummary,
) -> RedundantBodyPrognPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    RedundantBodyPrognPolicy {
        fail_on_violation: options.fail_on_violation(),
        implicit_progn_form_count: summary.implicit_progn_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progns(input: &str) -> (usize, Vec<RedundantBodyPrognItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_redundant_body_progns(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect redundant body progns")
    }

    #[test]
    fn flags_a_progn_body_of_when() {
        let (count, violations) = progns("(when c (progn a b))");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "when");
        assert_eq!(violations[0].body_form_count, 2);
    }

    #[test]
    fn body_span_isolates_the_inner_body_forms() {
        let input = "(unless done (progn a b c))";
        let (_, violations) = progns(input);
        let body = violations[0].body_span;
        assert_eq!(&input[body.start().get()..body.end().get()], "a b c");
    }

    #[test]
    fn flags_a_progn_body_of_let_and_defun() {
        let (_, violations) = progns("(let ((x 1)) (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "let");

        let (_, violations) = progns("(defun f (x) (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "defun");
    }

    #[test]
    fn flags_a_progn_after_a_declaration() {
        let (_, violations) = progns("(let ((x 1)) (declare (ignore x)) (progn a b))");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_single_form_progn() {
        // (progn x) is the redundant-progn rule's job, not this one.
        let (_, violations) = progns("(when c (progn x))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_in_the_test_position() {
        // The progn is the when test (index 1), a single value form, not a body.
        let (_, violations) = progns("(when (progn a b) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_as_a_let_binding_init() {
        let (_, violations) = progns("(let ((x (progn a b))) x)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_progn_inside_a_progn() {
        // That is the nested-progn rule's territory (parent is progn).
        let (count, violations) = progns("(progn (progn a b))");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_parent_head() {
        let (_, violations) = progns("(WHEN c (progn a b))");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].parent, "when");
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(when c (progn a b))", Dialect::Clojure)
            .expect("parse");
        let (count, violations) =
            collect_redundant_body_progns(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect redundant body progns");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = progns("(when c (progn a b))");
        let summary = summarize_redundant_body_progns(count, items);

        let quiet = evaluate_redundant_body_progn_policy(
            RedundantBodyPrognPolicyOptions::new(false),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_redundant_body_progn_policy(
            RedundantBodyPrognPolicyOptions::new(true),
            &summary,
        );
        assert!(!strict.passed);
    }
}
