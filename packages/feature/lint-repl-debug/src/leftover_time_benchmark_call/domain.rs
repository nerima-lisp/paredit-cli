//! `leftover-time-benchmark-call` detection: a Common Lisp `(time form)`
//! wrapper left in committed source.
//!
//! # Why the fix does not depend on position
//!
//! Per the CLHS `time` entry (`time form => result*`), `time` evaluates
//! `form`, prints timing data to `*trace-output*` as a side effect, and
//! **returns exactly the values `form` itself returns** — all of them, not
//! only the primary value. Unwrapping `(time form)` to `form` is therefore
//! value-preserving in *every* position a call can appear, including tail
//! position and a multiple-value context: the only observable difference is
//! the loss of the timing report, which is exactly the leftover artifact
//! this rule exists to remove. This is the one rule in the package (along
//! with `leftover-step-call`) whose fix does not need
//! `paredit-feature-lint-repl-debug::support`'s body-position analysis —
//! removal safety here does not depend on position at all, only on not being
//! inside quoted data (still handled by [`crate::support::walk_evaluated_forms`],
//! whose `RemovalSafety` this rule simply does not consult).
//!
//! What it *does* depend on is the head being `cl:time` at all: under
//! `(flet ((time (thunk) …)) (time fn))` the call is to the file's own local
//! function, and unwrapping it would return `fn` unevaluated instead of
//! calling it. Such an occurrence is still reported and simply carries no
//! fix — see [`crate::support::OperatorScope`].
//!
//! Only the exact one-argument shape `(time form)` is matched; `(time)` (no
//! form) and `(time a b)` (not valid `time` syntax) are left alone rather
//! than guessed at.
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_is};

use crate::support::{OperatorScope, walk_evaluated_forms};

#[derive(Debug, Clone)]
pub struct LeftoverTimeBenchmarkCallItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The wrapped form's own span — the fix's replacement text — or
    /// `None` when the head is not the operator it names (see
    /// [`crate::support::OperatorScope`]) and no fix is offered.
    pub form_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverTimeBenchmarkCallSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverTimeBenchmarkCallItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverTimeBenchmarkCallPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverTimeBenchmarkCallPolicyOptions {
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
pub struct LeftoverTimeBenchmarkCallPolicy {
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
    violations: &mut Vec<LeftoverTimeBenchmarkCallItem>,
) {
    walk_evaluated_forms(root, |view, _position| {
        *scanned_form_count += 1;
        if !is_paren_list(view) {
            return;
        }
        let Some(head) = list_head(view) else {
            return;
        };
        if !symbol_is(head, "time") {
            return;
        }
        // Exactly `(time form)`: head plus one argument.
        if view.children.len() != 2 {
            return;
        }
        let form_span = (!scope.head_is_locally_bound(view)).then(|| view.children[1].span);
        violations.push(LeftoverTimeBenchmarkCallItem {
            path: path.to_path_buf(),
            span: view.span,
            form_span,
        });
    });
}

pub fn collect_leftover_time_benchmark_call(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverTimeBenchmarkCallItem>)> {
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
pub const fn summarize_leftover_time_benchmark_call(
    scanned_form_count: usize,
    violations: Vec<LeftoverTimeBenchmarkCallItem>,
) -> LeftoverTimeBenchmarkCallSummary {
    LeftoverTimeBenchmarkCallSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_time_benchmark_call_policy(
    options: LeftoverTimeBenchmarkCallPolicyOptions,
    summary: &LeftoverTimeBenchmarkCallSummary,
) -> LeftoverTimeBenchmarkCallPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverTimeBenchmarkCallPolicy {
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

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverTimeBenchmarkCallItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_time_benchmark_call(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_time_benchmark_call");
        violations
    }

    #[test]
    fn flags_a_bare_time_call() {
        let violations = violations_in("(time (compute))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_time_with_no_argument_or_too_many() {
        assert!(violations_in("(time)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(time a b)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn does_not_flag_time_used_as_data() {
        assert!(violations_in("(fboundp 'time)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn is_flagged_and_fixed_even_in_tail_position() {
        // Unlike the removal rules, `time` unwraps unconditionally: CLHS
        // guarantees it returns exactly `form`'s values, so tail position is
        // just as safe as any other.
        let input = "(defun f ()\n  (time (compute)))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_time_benchmark_call(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
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
        assert!(violations_in("'(time (compute))", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (time (compute)))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_call_inside_a_string_literal_is_not_flagged() {
        assert!(violations_in("(princ \"(time (compute))\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_string_argument_with_escapes_is_preserved_verbatim_by_the_replacement() {
        let input = "(time (princ \"a\\nb \\\"c\\\" \\\\ d\"))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_time_benchmark_call(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect");
        let item = &violations[0];
        let form_span = item.form_span.expect("fix");
        let mut rewritten = input.to_owned();
        let replacement = input[form_span.start().get()..form_span.end().get()].to_owned();
        rewritten.replace_range(item.span.start().get()..item.span.end().get(), &replacement);
        assert_eq!(rewritten, "(princ \"a\\nb \\\"c\\\" \\\\ d\")");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp).expect("must still parse");
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque() {
        assert!(violations_in("#+sbcl (time (compute))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in("(defun f () (time (compute)))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn a_crlf_file_computes_a_correct_form_span() {
        let input = "(defun f ()\r\n  (time (compute)))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) = collect_leftover_time_benchmark_call(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("collect");
        let item = &violations[0];
        let form_span = item.form_span.expect("fix");
        assert_eq!(
            &input[form_span.start().get()..form_span.end().get()],
            "(compute)"
        );
    }

    #[test]
    fn a_call_to_a_locally_bound_flet_operator_is_reported_with_no_fix() {
        // The head is the file's own `flet` function/macro, not the
        // dialect's `time` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun run (fn)\n  (flet ((time (thunk) (funcall thunk)))\n    (time fn)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(shadowed.len(), 1, "the finding is still reported");
        assert!(
            shadowed[0].form_span.is_none(),
            "a shadowed operator must carry no fix"
        );

        // The same shape with the `flet` binding renamed is the
        // ordinary case, and still gets its fix — without this the assertion
        // above would pass for the wrong reason.
        let unshadowed = violations_in(
            "(defun run (fn)\n  (flet ((clock (thunk) (funcall thunk)))\n    (time fn)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].form_span.is_some());
    }

    #[test]
    fn a_backquoted_time_shape_without_unquote_is_not_flagged() {
        // A backquote with no unquote is unevaluated data, so the shape
        // inside it is a list to build, not a call to remove.
        assert!(violations_in("`(a (time (compute)) b)", Dialect::CommonLisp).is_empty());
    }
}
