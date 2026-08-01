//! `leftover-inspect-call` detection: a Common Lisp `(inspect x)` or
//! `(describe x)` left in committed source — both open an interactive
//! browser/print a description to `*standard-output*` for a human, which has
//! no business running unattended.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};

use crate::support::{EvaluatedCandidate, OperatorScope, RemovalSafety, compute_evaluated_forms};

pub(crate) const HEADS: [&str; 2] = ["inspect", "describe"];

#[derive(Debug, Clone)]
pub struct LeftoverInspectCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub head: String,
    pub fix_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverInspectCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverInspectCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverInspectCallPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverInspectCallPolicyOptions {
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
pub struct LeftoverInspectCallPolicy {
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
    violations: &mut Vec<LeftoverInspectCallItem>,
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
        violations.push(LeftoverInspectCallItem {
            path: path.to_path_buf(),
            span: candidate.view.span,
            head: head.to_owned(),
            fix_span,
        });
    }
}

pub fn collect_leftover_inspect_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverInspectCallItem>)> {
    if dialect != Dialect::CommonLisp {
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
pub const fn summarize_leftover_inspect_call(
    scanned_form_count: usize,
    violations: Vec<LeftoverInspectCallItem>,
) -> LeftoverInspectCallSummary {
    LeftoverInspectCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_inspect_call_policy(
    options: LeftoverInspectCallPolicyOptions,
    summary: &LeftoverInspectCallSummary,
) -> LeftoverInspectCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverInspectCallPolicy {
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

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverInspectCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_inspect_call(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_inspect_call");
        violations
    }

    #[test]
    fn flags_inspect_and_describe() {
        assert_eq!(violations_in("(inspect x)", Dialect::CommonLisp).len(), 1);
        assert_eq!(violations_in("(describe x)", Dialect::CommonLisp).len(), 1);
    }

    #[test]
    fn does_not_flag_used_as_data() {
        assert!(violations_in("(fboundp 'inspect)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn an_unmodelled_dialect_is_left_alone() {
        assert!(violations_in("(inspect x)", Dialect::Racket).is_empty());
    }

    #[test]
    fn the_last_form_of_a_body_gets_no_fix() {
        let violations = violations_in("(defun f (x) (inspect x))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_none());
    }

    #[test]
    fn a_non_last_body_form_gets_a_fix_that_leaves_valid_source() {
        let input = "(defun f (x)\n  (describe x)\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_inspect_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f (x)\n  (+ 1 2))");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_quoted_shape_is_not_flagged() {
        assert!(violations_in("'(inspect x)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (describe x))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_flagged() {
        assert!(violations_in("(princ \"(inspect x)\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_with_escapes_survives_the_fix_untouched() {
        let input = "(defun f (x)\n  (inspect x)\n  (princ \"a\\nb \\\"c\\\" \\\\ d\"))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_inspect_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(
            rewritten,
            "(defun f (x)\n  (princ \"a\\nb \\\"c\\\" \\\\ d\"))"
        );
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque() {
        assert!(violations_in("#+sbcl (inspect x)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in("(defun f (x) (inspect x) (+ 1 2))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn a_crlf_file_computes_a_correct_fix_span() {
        let input = "(defun f (x)\r\n  (inspect x)\r\n  (+ 1 2))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_inspect_call(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f (x)\r\n  (+ 1 2))\r\n");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_call_to_a_locally_bound_labels_operator_is_reported_with_no_fix() {
        // The head is the file's own `labels` function/macro, not the
        // dialect's `inspect` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun f (x)\n  (labels ((inspect (v) v))\n    (inspect x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].fix_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `labels` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun f (x)\n  (labels ((render (v) v))\n    (inspect x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].fix_span.is_some());
    }

    #[test]
    fn a_backquoted_inspect_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (inspect x) b)", Dialect::CommonLisp).is_empty());
    }
}
