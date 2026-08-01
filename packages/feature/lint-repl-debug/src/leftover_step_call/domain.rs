//! `leftover-step-call` detection: a Common Lisp `(step form)` wrapper left
//! in committed source.
//!
//! # Why the fix does not depend on position
//!
//! Per the CLHS `step` entry (`step form => result*`), `step` evaluates
//! `form` with single-stepping enabled as a side effect, and **returns
//! exactly the values `form` itself returns**. Unwrapping `(step form)` to
//! `form` is therefore value-preserving in every position, the same
//! reasoning as `leftover-time-benchmark-call` — see that rule's module doc
//! for the fuller explanation, which applies here unchanged with `step` in
//! place of `time`.
//!
//! What it does depend on is the head being `cl:step` at all: under
//! `(labels ((step (k) …)) (step n))` the call is to the file's own local
//! function, and unwrapping it would delete that call. Such an occurrence is
//! still reported and simply carries no fix — see
//! [`crate::support::OperatorScope`].
//!
//! Only the exact one-argument shape `(step form)` is matched.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::{OperatorScope, walk_evaluated_forms};

#[derive(Debug, Clone)]
pub struct LeftoverStepCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The wrapped form's own span — the fix's replacement text — or `None`
    /// when the head is not the operator it names (see
    /// [`crate::support::OperatorScope`]) and no fix is offered.
    pub form_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverStepCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverStepCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverStepCallPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverStepCallPolicyOptions {
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
pub struct LeftoverStepCallPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine(
    root: &ExpressionView,
    scope: &OperatorScope<'_>,
    path: &Path,
    scanned_form_count: &mut usize,
    violations: &mut Vec<LeftoverStepCallItem>,
) {
    walk_evaluated_forms(root, |view, _position| {
        *scanned_form_count += 1;
        if !is_paren_list(view) {
            return;
        }
        let Some(head) = list_head(view) else {
            return;
        };
        if !symbol_is(head, "step") {
            return;
        }
        if view.children.len() != 2 {
            return;
        }
        let form_span = (!scope.head_is_locally_bound(view)).then(|| view.children[1].span);
        violations.push(LeftoverStepCallItem {
            path: path.to_path_buf(),
            span: view.span,
            form_span,
        });
    });
}

pub fn collect_leftover_step_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverStepCallItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }
    let mut scanned_form_count = 0;
    let mut violations = Vec::new();
    examine(
        &tree.root_view(),
        &OperatorScope::standalone(dialect, tree),
        path,
        &mut scanned_form_count,
        &mut violations,
    );
    Ok((scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_leftover_step_call(
    scanned_form_count: usize,
    violations: Vec<LeftoverStepCallItem>,
) -> LeftoverStepCallSummary {
    LeftoverStepCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_step_call_policy(
    options: LeftoverStepCallPolicyOptions,
    summary: &LeftoverStepCallSummary,
) -> LeftoverStepCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverStepCallPolicy {
        fail_on_violation: options.fail_on_violation(),
        scanned_form_count: summary.scanned_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverStepCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_step_call(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_step_call");
        violations
    }

    #[test]
    fn flags_a_bare_step_call() {
        assert_eq!(
            violations_in("(step (compute))", Dialect::CommonLisp).len(),
            1
        );
    }

    #[test]
    fn does_not_flag_step_with_no_argument_or_too_many() {
        assert!(violations_in("(step)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(step a b)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_step_used_as_a_lambda_list_parameter() {
        // The exact false-positive trap `support::binding_position` exists
        // for: `step` is a common loop-variable/parameter name.
        assert!(violations_in("(defun f (step) step)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_step_used_as_a_let_binding_name() {
        assert!(violations_in("(let ((step 0)) step)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn is_flagged_and_fixed_even_in_tail_position() {
        let input = "(defun f ()\n  (step (compute)))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_step_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        assert_eq!(violations.len(), 1);
        let form_span = violations[0].form_span.expect("fix");
        assert_eq!(
            &input[form_span.start().get()..form_span.end().get()],
            "(compute)"
        );
    }

    #[test]
    fn a_quoted_shape_is_not_flagged() {
        assert!(violations_in("'(step (compute))", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (step (compute)))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_flagged() {
        assert!(violations_in("(princ \"(step (compute))\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_argument_with_escapes_is_preserved_verbatim_by_the_replacement() {
        let input = "(step (princ \"a\\nb \\\"c\\\" \\\\ d\"))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_step_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let item = &violations[0];
        let form_span = item.form_span.expect("fix");
        let replacement = input[form_span.start().get()..form_span.end().get()].to_owned();
        let mut rewritten = input.to_owned();
        rewritten.replace_range(item.span.start().get()..item.span.end().get(), &replacement);
        assert_eq!(rewritten, "(princ \"a\\nb \\\"c\\\" \\\\ d\")");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque() {
        assert!(violations_in("#+sbcl (step (compute))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        assert_eq!(
            violations_in("(defun f () (step (compute)))", Dialect::CommonLisp).len(),
            1
        );
    }

    #[test]
    fn a_crlf_file_computes_a_correct_form_span() {
        let input = "(defun f ()\r\n  (step (compute)))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_step_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let item = &violations[0];
        let form_span = item.form_span.expect("fix");
        assert_eq!(
            &input[form_span.start().get()..form_span.end().get()],
            "(compute)"
        );
    }

    #[test]
    fn a_call_to_a_locally_bound_labels_operator_is_reported_with_no_fix() {
        // The head is the file's own `labels` function/macro, not the
        // dialect's `step` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun run (n)\n  (labels ((step (k) (1+ k)))\n    (step n)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].form_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `labels` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun run (n)\n  (labels ((advance (k) (1+ k)))\n    (step n)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].form_span.is_some());
    }

    #[test]
    fn a_backquoted_step_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (step (compute)) b)", Dialect::CommonLisp).is_empty());
    }
}
