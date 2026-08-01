//! `leftover-break-call` detection: a Common Lisp `(break ...)` left in
//! committed source — an interactive-debugger entry point that has no
//! business running unattended.
//!
//! Scope: Common Lisp only. Racket also has a `break`, with unrelated
//! (loop-exit) semantics — this rule deliberately does not extend to it.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::{OperatorScope, RemovalSafety, removal_span, walk_evaluated_forms};

#[derive(Debug, Clone)]
pub struct LeftoverBreakCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub fix_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverBreakCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverBreakCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverBreakCallPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverBreakCallPolicyOptions {
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
pub struct LeftoverBreakCallPolicy {
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
    violations: &mut Vec<LeftoverBreakCallItem>,
) {
    walk_evaluated_forms(root, |view, position| {
        *scanned_form_count += 1;
        if !is_paren_list(view) {
            return;
        }
        let Some(head) = list_head(view) else {
            return;
        };
        if !symbol_is(head, "break") {
            return;
        }
        let fix_span = (matches!(position.safety, RemovalSafety::Safe)
            && !scope.head_is_locally_bound(view))
        .then(|| removal_span(position, view.span));
        violations.push(LeftoverBreakCallItem {
            path: path.to_path_buf(),
            span: view.span,
            fix_span,
        });
    });
}

pub fn collect_leftover_break_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverBreakCallItem>)> {
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
pub const fn summarize_leftover_break_call(
    scanned_form_count: usize,
    violations: Vec<LeftoverBreakCallItem>,
) -> LeftoverBreakCallSummary {
    LeftoverBreakCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_break_call_policy(
    options: LeftoverBreakCallPolicyOptions,
    summary: &LeftoverBreakCallSummary,
) -> LeftoverBreakCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverBreakCallPolicy {
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

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverBreakCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_break_call(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_break_call");
        violations
    }

    #[test]
    fn flags_a_bare_break_call() {
        assert_eq!(violations_in("(break)", Dialect::CommonLisp).len(), 1);
        assert_eq!(
            violations_in("(break \"~a\" x)", Dialect::CommonLisp).len(),
            1
        );
    }

    #[test]
    fn does_not_extend_to_racket_break() {
        assert!(violations_in("(break)", Dialect::Racket).is_empty());
    }

    #[test]
    fn does_not_flag_break_used_as_data() {
        assert!(violations_in("(fboundp 'break)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn the_last_form_of_a_body_gets_no_fix() {
        let violations = violations_in("(defun f () (break))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_none());
    }

    #[test]
    fn a_non_last_body_form_gets_a_fix_that_leaves_valid_source() {
        let input = "(defun f ()\n  (break)\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_break_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_quoted_break_shape_is_not_flagged() {
        assert!(violations_in("'(break)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (break))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_break_call_inside_a_string_literal_is_not_flagged() {
        assert!(violations_in("(princ \"(break)\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_with_escapes_survives_the_fix_untouched() {
        let input = "(defun f ()\n  (break)\n  (princ \"a\\nb \\\"c\\\" \\\\ d\"))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_break_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(
            rewritten,
            "(defun f ()\n  (princ \"a\\nb \\\"c\\\" \\\\ d\"))"
        );
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque() {
        assert!(violations_in("#+sbcl (break)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in("(defun f () (break) (+ 1 2))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn a_crlf_file_computes_a_correct_fix_span() {
        let input = "(defun f ()\r\n  (break)\r\n  (+ 1 2))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_break_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\r\n  (+ 1 2))\r\n");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_call_to_a_locally_bound_macrolet_operator_is_reported_with_no_fix() {
        // The head is the file's own `macrolet` function/macro, not the
        // dialect's `break` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun f (x)\n  (macrolet ((break (v) `(handle-break ,v)))\n    (break x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].fix_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `macrolet` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun f (x)\n  (macrolet ((guard (v) `(handle-break ,v)))\n    (break x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].fix_span.is_some());
    }

    #[test]
    fn a_backquoted_break_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (break) b)", Dialect::CommonLisp).is_empty());
    }
}
