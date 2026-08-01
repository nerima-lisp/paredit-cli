//! `leftover-print-debug` detection: a bare debug-print call left in
//! committed source — `(print x)`, `(message "got here")`,
//! `(println debug-value)`, and their per-dialect siblings.
//!
//! Each dialect has its own debug-print vocabulary — this is the one rule in
//! the package that is genuinely multi-dialect, so [`heads_for`] is a real
//! `Dialect -> &[&str]` table rather than one flat list assumed to apply
//! everywhere (the "dialect-blind operator table" mistake this project's own
//! history warns about).
//!
//! Removal safety (whether a fix is offered at all) is
//! [`paredit_feature_lint_repl_debug::support`]'s job: a flagged call is only
//! auto-removed when it is a bare top-level form or a non-last form of its
//! enclosing implicit-progn/`cond`-clause body.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, SyntaxTree};
use paredit_core_syntax::view_query::{list_head, symbol_in};

use crate::support::{EvaluatedCandidate, OperatorScope, RemovalSafety, compute_evaluated_forms};

/// The debug-print head symbols this rule recognizes for `dialect`, or `&[]`
/// for a dialect it does not model at all.
#[must_use]
pub const fn heads_for(dialect: Dialect) -> &'static [&'static str] {
    match dialect {
        Dialect::CommonLisp => &["princ", "print", "prin1", "pprint"],
        Dialect::EmacsLisp => &["message"],
        Dialect::Clojure => &["println", "prn"],
        Dialect::Scheme => &["display"],
        Dialect::Racket => &["displayln"],
        Dialect::Janet => &["print", "pp"],
        Dialect::Fennel => &["print"],
        Dialect::Hy => &["print"],
        _ => &[],
    }
}

#[derive(Debug, Clone)]
pub struct LeftoverPrintDebugItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    /// The head symbol as written (before case-folding), for the message.
    pub head: String,
    /// The exact span a fix would replace with `""`, or `None` when this call
    /// is not provably safe to remove outright (it is still reported).
    pub fix_span: Option<ByteSpan>,
}

#[derive(Debug)]
pub struct LeftoverPrintDebugSummary {
    pub scanned_form_count: usize,
    pub violations: Vec<LeftoverPrintDebugItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LeftoverPrintDebugPolicyOptions {
    fail_on_violation: bool,
}

impl LeftoverPrintDebugPolicyOptions {
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
pub struct LeftoverPrintDebugPolicy {
    pub fail_on_violation: bool,
    pub scanned_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines every already-computed evaluated-code candidate in `candidates`
/// — see [`crate::support::evaluated_candidates`], which is what makes this
/// package's seven sharing rules cost one tree walk per file rather than
/// seven — reporting each call to one of `dialect`'s debug-print heads.
pub fn examine(
    candidates: &[EvaluatedCandidate],
    scope: &OperatorScope<'_>,
    dialect: Dialect,
    path: &Path,
    violations: &mut Vec<LeftoverPrintDebugItem>,
) {
    let heads = heads_for(dialect);
    if heads.is_empty() {
        return;
    }
    for candidate in candidates {
        let Some(head) = list_head(&candidate.view) else {
            continue;
        };
        if !symbol_in(head, heads) {
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
        violations.push(LeftoverPrintDebugItem {
            path: path.to_path_buf(),
            span: candidate.view.span,
            head: head.to_owned(),
            fix_span,
        });
    }
}

/// Collects every violation across a whole file, along with the total number
/// of forms scanned. A dialect [`heads_for`] does not model reports zero
/// scanned forms and no violations — indistinguishable, at this rule's own
/// report shape, from a modelled dialect with nothing to flag; the per-rule
/// dialect-support matrix (`inspect leftover-print-debug --help`, and this
/// crate's `dialect_scope`) is the place that distinction is authoritative.
pub fn collect_leftover_print_debug(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<LeftoverPrintDebugItem>)> {
    if heads_for(dialect).is_empty() {
        return Ok((0, Vec::new()));
    }
    let forms = compute_evaluated_forms(&tree.root_view());
    let mut violations = Vec::new();
    examine(
        &forms.candidates,
        &OperatorScope::standalone(dialect, tree),
        dialect,
        path,
        &mut violations,
    );
    Ok((forms.scanned_form_count, violations))
}

#[must_use]
pub const fn summarize_leftover_print_debug(
    scanned_form_count: usize,
    violations: Vec<LeftoverPrintDebugItem>,
) -> LeftoverPrintDebugSummary {
    LeftoverPrintDebugSummary {
        scanned_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_leftover_print_debug_policy(
    options: LeftoverPrintDebugPolicyOptions,
    summary: &LeftoverPrintDebugSummary,
) -> LeftoverPrintDebugPolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LeftoverPrintDebugPolicy {
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

    fn violations_in(input: &str, dialect: Dialect) -> Vec<LeftoverPrintDebugItem> {
        let tree = SyntaxTree::parse_with_dialect(input, dialect).expect("parse");
        let (_, violations) =
            collect_leftover_print_debug(&PathBuf::from("test.lisp"), dialect, &tree)
                .expect("collect leftover_print_debug");
        violations
    }

    #[test]
    fn flags_a_bare_top_level_print_call() {
        let violations = violations_in("(print x)", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "print");
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn flags_every_common_lisp_debug_print_head() {
        for head in ["princ", "print", "prin1", "pprint"] {
            let violations = violations_in(&format!("({head} x)"), Dialect::CommonLisp);
            assert_eq!(violations.len(), 1, "{head} not flagged");
        }
    }

    #[test]
    fn does_not_flag_an_unrelated_call() {
        assert!(violations_in("(format t \"~a\" x)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn flags_message_in_emacs_lisp_only() {
        assert_eq!(
            violations_in("(message \"got here\")", Dialect::EmacsLisp).len(),
            1
        );
        assert!(violations_in("(message \"got here\")", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn flags_the_clojure_and_scheme_and_racket_and_janet_and_fennel_and_hy_heads() {
        assert_eq!(violations_in("(println x)", Dialect::Clojure).len(), 1);
        assert_eq!(violations_in("(prn x)", Dialect::Clojure).len(), 1);
        assert_eq!(violations_in("(display x)", Dialect::Scheme).len(), 1);
        assert_eq!(violations_in("(displayln x)", Dialect::Racket).len(), 1);
        assert_eq!(violations_in("(print x)", Dialect::Janet).len(), 1);
        assert_eq!(violations_in("(pp x)", Dialect::Janet).len(), 1);
        assert_eq!(violations_in("(print x)", Dialect::Fennel).len(), 1);
        assert_eq!(violations_in("(print x)", Dialect::Hy).len(), 1);
    }

    #[test]
    fn an_unmodelled_dialect_is_reported_as_such() {
        let tree = SyntaxTree::parse_with_dialect("(print x)", Dialect::Lfe).expect("parse");
        let (count, violations) =
            collect_leftover_print_debug(&PathBuf::from("test.lfe"), Dialect::Lfe, &tree)
                .expect("collect");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn the_last_form_of_a_body_is_flagged_with_no_fix() {
        let violations = violations_in("(defun f () (print x))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_none());
    }

    #[test]
    fn a_non_last_body_form_gets_a_fix_that_leaves_valid_source() {
        let input = "(defun f ()\n  (print x)\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_print_debug(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        assert_eq!(violations.len(), 1);
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("the fixed source must still parse");
    }

    #[test]
    fn a_quoted_print_shape_is_not_flagged() {
        assert!(violations_in("'(print x)", Dialect::CommonLisp).is_empty());
        assert!(violations_in("(quote (print x))", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_backquoted_print_shape_without_unquote_is_not_flagged() {
        assert!(violations_in("`(a (print x) b)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_print_shaped_lambda_list_parameter_is_not_flagged() {
        assert!(violations_in("(defun f (print) print)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_print_call_inside_a_string_literal_is_not_flagged() {
        // The text "(print x)" appears only inside a string; the reader
        // never produces a `print` call node for it.
        let violations = violations_in("(princ \"(print x)\")", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].head, "princ");
    }

    #[test]
    fn a_string_argument_with_escapes_and_a_newline_is_not_corrupted_by_the_fix() {
        let input = "(defun f ()\n  (print \"line1\\nline2 \\\"quoted\\\" \\\\ end\")\n  (+ 1 2))";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_print_debug(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        assert_eq!(violations.len(), 1);
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\n  (+ 1 2))");
    }

    #[test]
    fn a_reader_conditional_wrapped_call_is_opaque_and_not_flagged() {
        // `#+sbcl (print x)` folds into a single atom under the dialect-aware
        // parser, so there is no `print`-headed list node to find at all.
        assert!(violations_in("#+sbcl (print x)", Dialect::CommonLisp).is_empty());
    }

    #[test]
    fn a_one_line_definition_is_handled() {
        let violations = violations_in("(defun f () (print x) (+ 1 2))", Dialect::CommonLisp);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].fix_span.is_some());
    }

    #[test]
    fn a_crlf_file_still_computes_a_correct_fix_span() {
        let input = "(defun f ()\r\n  (print x)\r\n  (+ 1 2))\r\n";
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let (_, violations) =
            collect_leftover_print_debug(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
                .expect("collect");
        assert_eq!(violations.len(), 1);
        let fix_span = violations[0].fix_span.expect("fix");
        let mut rewritten = input.to_owned();
        rewritten.replace_range(fix_span.start().get()..fix_span.end().get(), "");
        assert_eq!(rewritten, "(defun f ()\r\n  (+ 1 2))\r\n");
        SyntaxTree::parse_with_dialect(&rewritten, Dialect::CommonLisp)
            .expect("the fixed source must still parse");
    }

    #[test]
    fn a_call_to_a_locally_bound_flet_operator_is_reported_with_no_fix() {
        // The head is the file's own `flet` function/macro, not the
        // dialect's `print` — the call is real code, so a fix that rewrote it
        // would silently delete a call the author wrote.
        let shadowed = violations_in(
            "(defun f (x)\n  (flet ((print (v) v))\n    (print x)\n    (foo)))",
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
            "(defun f (x)\n  (flet ((render (v) v))\n    (print x)\n    (foo)))",
            Dialect::CommonLisp,
        );
        assert_eq!(unshadowed.len(), 1);
        assert!(unshadowed[0].fix_span.is_some());
    }
}
