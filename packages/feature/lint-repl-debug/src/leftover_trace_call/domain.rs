//! `leftover-trace-call` detection: `trace`/`untrace` used as a statement —
//! `(trace my-function)`, `(untrace)` — left in committed source.
//!
//! Only a *call* head is matched (`list_head(view)`), so `(fboundp 'trace)`
//! or a variable named `trace` is never flagged: `trace` there is data or a
//! reference, not the head of a call.
//!
//! Scope: Common Lisp and Emacs Lisp, both of which have `trace`/`untrace` as
//! debugging macros with this shape.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};

use crate::support::{EvaluatedCandidate, OperatorScope, RemovalSafety, compute_evaluated_forms};

pub(crate) const HEADS: [&str; 2] = ["trace", "untrace"];

#[derive(Debug, Clone)]
pub struct LeftoverTraceCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub fix_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverTraceCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverTraceCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverTraceCallPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverTraceCallPolicyOptions {
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
pub struct LeftoverTraceCallPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

pub fn examine(
    candidates: &[EvaluatedCandidate],
    scope: &OperatorScope<'_>,
    path: &Path,
    violations: &mut Vec<LeftoverTraceCallItem>,
) {
    for candidate in candidates {
        let Some(head) = list_head(&candidate.view) else {
            continue;
        };
        if !symbol_in(head, &HEADS) {
            continue;
        }
        let locally_bound = candidate
            .head_symbol_span
            .is_some_and(|span| scope.symbol_span_is_locally_bound(span));
        let fix_span =
            (matches!(candidate.safety, RemovalSafety::Safe) && !locally_bound).then(|| {
                candidate
                    .removal_span
                    .expect("Safe candidate carries a removal span")
            });
        violations.push(LeftoverTraceCallItem {
            path: path.to_path_buf(),
            span: candidate.view.span,
            head: head.to_owned(),
            fix_span,
        });
    }
}

pub fn collect_leftover_trace_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverTraceCallItem>)> {
    if !matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp) {
        return Ok((0, Vec::new()));
    }
    let forms = compute_evaluated_forms(&tree.root_view());
    let mut violations = Vec::new();
    examine(
        &forms.candidates,
        &OperatorScope::standalone(dialect, tree),
        path,
        &mut violations,
    );
    Ok((forms.scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_leftover_trace_call(
    scanned_form_count: usize,
    violations: Vec<LeftoverTraceCallItem>,
) -> LeftoverTraceCallSummary {
    LeftoverTraceCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_trace_call_policy(
    options: LeftoverTraceCallPolicyOptions,
    summary: &LeftoverTraceCallSummary,
) -> LeftoverTraceCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverTraceCallPolicy {
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

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverTraceCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_trace_call(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_trace_call");
        violations
    }

    #[test]
    fn flags_a_bare_trace_call() {
        let violations = violations_in("(trace my-function)", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "trace");
    }

    #[test]
    fn flags_untrace_in_emacs_lisp() {
        assert_eq!(violations_in("(untrace)", Dialect::EmacsLisp).len(), 1);
    }

    #[test]
    fn does_not_flag_trace_used_as_data() {
        assert!(violations_in("(fboundp 'trace)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(list trace untrace)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_is_left_alone() {
        assert!(violations_in("(trace foo)", Dialect::Clojure).is_empty());
    }

    #[test]
    fn the_last_form_of_a_body_gets_no_fix() {
        let violations = violations_in("(defun f () (trace f))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_none());
    }

    #[test]
    fn a_non_last_body_form_gets_a_fix_that_leaves_valid_source() {
        let input = "(defun f ()\n  (trace f)\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_trace_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_quoted_trace_shape_is_not_flagged() {
        assert!(violations_in("'(trace f)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (trace f))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_trace_call_inside_a_string_literal_is_not_flagged() {
        let violations = violations_in("(princ \"(trace f)\")", Dialect::CommonLisp);
        assert!(violations.is_empty());
    }

    #[test]
    fn a_string_with_escapes_survives_the_fix_untouched() {
        let input = "(defun f ()\n  (trace f)\n  (princ \"a\\nb \\\"c\\\" \\\\ d\"))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_trace_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
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
        assert!(violations_in("#+sbcl (trace f)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in("(defun f () (trace f) (+ 1 2))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn a_crlf_file_computes_a_correct_fix_span() {
        let input = "(defun f ()\r\n  (trace f)\r\n  (+ 1 2))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_trace_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\r\n  (+ 1 2))\r\n");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_call_to_a_locally_bound_flet_operator_is_reported_with_no_fix() {
        // The head is the file's own `flet` function/macro, not the
        // dialect's `trace` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun f (x)\n  (flet ((trace (v) v))\n    (trace x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].fix_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `flet` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun f (x)\n  (flet ((render (v) v))\n    (trace x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].fix_span.is_some());
    }

    #[test]
    fn a_backquoted_trace_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (trace foo) b)", Dialect::CommonLisp).is_empty());
    }
}
