//! `handler-bind` handlers that end by producing a value.
//!
//! This is the difference between `handler-case` and `handler-bind` that costs
//! people an afternoon. A `handler-case` clause's body *is* the value of the
//! whole form. A `handler-bind` handler runs in the dynamic context of the
//! signal, and its return value is discarded — returning normally means
//! *declining* to handle, and the condition carries on to the next handler.
//! So `(handler-bind ((parse-error (lambda (c) (list :failed c)))) …)` looks
//! like it produces `(:failed …)` and in fact produces nothing at all.
//!
//! Deliberately narrow. Only a *bare value* is flagged: a literal or variable
//! reference, a quoted form, or a call to one of the pure constructors whose
//! entire purpose is to build a value. A handler ending in `(format …)`,
//! `(log-it c)` or any other call is left alone — declining after a side effect
//! is a real and correct idiom, and flagging it would make the rule noise. What
//! is left is the shape that can only have been meant as a result.
//!
//! A handler that is not a literal `lambda` (`#'my-handler`, a variable) is
//! opaque here and is never flagged: its body is not in this form.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionKind, ExpressionView, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, calls, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, symbol_name};

#[derive(Debug, Clone)]
pub struct HandlerBindHandlerReturnsBareValueItem {
    /// The span of the discarded trailing form, which is the thing to change.
    pub span: ByteSpan,
    /// The condition type the handler was bound for.
    pub condition_type: String,
}

impl Finding for HandlerBindHandlerReturnsBareValueItem {
    fn kind(&self) -> &'static str {
        "handler-bind-handler-returns-bare-value"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("condition={}", self.condition_type)]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("condition", json!(self.condition_type))]
    }

    fn message(&self) -> String {
        format!(
            "handler for `{}` ends in a bare value, which handler-bind discards; returning \
             normally declines to handle — invoke a restart, return-from, or resignal instead",
            self.condition_type
        )
    }
}

/// Calls whose whole purpose is to build a value, so that ending a handler with
/// one cannot have been meant as a side effect.
///
/// Kept short on purpose: every name added here is a claim that a call to it
/// does nothing but produce a result.
const VALUE_CONSTRUCTORS: [&str; 8] = [
    "list",
    "list*",
    "cons",
    "values",
    "vector",
    "make-list",
    "copy-list",
    "quote",
];

/// Unwraps `#'(lambda …)` / `(function (lambda …))` to the `lambda` inside.
fn as_lambda(handler: &ExpressionView) -> Option<&ExpressionView> {
    if !is_paren_list(handler) {
        return None;
    }
    if list_head(handler).is_some_and(|head| symbol_is(head, "function")) {
        return handler.children.get(1).and_then(as_lambda);
    }
    // `#'` is a reader prefix on the list itself, so the head is still `lambda`.
    list_head(handler)
        .is_some_and(|head| symbol_is(head, "lambda"))
        .then_some(handler)
}

/// Whether a form is a value and nothing else.
fn is_bare_value(form: &ExpressionView) -> bool {
    if form.kind == ExpressionKind::Atom {
        // A reader conditional reads together with the form after it, so what
        // the handler actually ends with is build-dependent and no claim can be
        // made about it.
        return !is_reader_conditional(form);
    }
    if !is_paren_list(form) {
        return false;
    }
    // `'(a b)` and `` `(a b) `` are lists carrying a quote prefix, and both are
    // literal data.
    if form
        .reader_prefixes
        .iter()
        .any(|prefix| matches!(prefix, ReaderPrefix::Quote | ReaderPrefix::Quasiquote))
    {
        return true;
    }
    calls(form, &VALUE_CONSTRUCTORS)
}

fn is_reader_conditional(form: &ExpressionView) -> bool {
    atom_text(form).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

/// The last form of a `lambda`'s body, past its lambda list, or `None` for a
/// body with nothing in it.
fn handler_body_tail(lambda: &ExpressionView) -> Option<&ExpressionView> {
    // children[0] is `lambda`, children[1] is the lambda list.
    lambda.children.get(2..).and_then(<[_]>::last)
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_handler_bind(
    view: &ExpressionView,
    handler_count: &mut usize,
    violations: &mut Vec<HandlerBindHandlerReturnsBareValueItem>,
) {
    if !is_paren_list(view) || !list_head(view).is_some_and(|head| symbol_is(head, "handler-bind"))
    {
        return;
    }
    let Some(bindings) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(bindings) {
        return;
    }

    for binding in &bindings.children {
        if !is_paren_list(binding) {
            continue;
        }
        let Some(condition_type) = binding.children.first().and_then(symbol_name) else {
            continue;
        };
        let Some(handler) = binding.children.get(1) else {
            continue;
        };
        *handler_count += 1;

        let Some(lambda) = as_lambda(handler) else {
            continue;
        };
        // An empty handler body returns nil too, but that is a handler that does
        // nothing at all rather than one that computes a discarded result, and
        // it is not what this rule is about.
        let Some(tail) = handler_body_tail(lambda) else {
            continue;
        };
        if is_bare_value(tail) {
            violations.push(HandlerBindHandlerReturnsBareValueItem {
                span: tail.span,
                condition_type,
            });
        }
    }
}

/// Collects every value-returning `handler-bind` handler in one file, with the
/// number of handlers scanned as the denominator beside them.
pub fn build_handler_bind_handler_returns_bare_value_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<HandlerBindHandlerReturnsBareValueItem>> {
    let mut handler_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for_each_evaluated_subview(&tree.root_view(), |view| {
            examine_handler_bind(view, &mut handler_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("handler_count", json!(handler_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<HandlerBindHandlerReturnsBareValueItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_handler_bind_handler_returns_bare_value_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn violations(input: &str) -> Vec<HandlerBindHandlerReturnsBareValueItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_handler_ending_in_a_literal() {
        let found = violations("(handler-bind ((parse-error (lambda (c) nil))) (run))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].condition_type, "parse-error");
    }

    #[test]
    fn flags_a_handler_ending_in_a_value_constructor() {
        let found =
            violations("(handler-bind ((parse-error (lambda (c) (list :failed c)))) (run))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn flags_a_handler_ending_in_a_quoted_form() {
        assert_eq!(
            violations("(handler-bind ((parse-error (lambda (c) 'failed))) (run))").len(),
            1
        );
    }

    #[test]
    fn flags_a_handler_ending_in_a_variable_reference() {
        assert_eq!(
            violations("(handler-bind ((parse-error (lambda (c) (log-it c) c))) (run))").len(),
            1
        );
    }

    #[test]
    fn reads_through_the_function_reader_macro() {
        assert_eq!(
            violations("(handler-bind ((parse-error #'(lambda (c) nil))) (run))").len(),
            1
        );
        assert_eq!(
            violations("(handler-bind ((parse-error (function (lambda (c) nil)))) (run))").len(),
            1
        );
    }

    /// The near miss: the handler that actually transfers control, which is
    /// what a `handler-bind` handler is for.
    #[test]
    fn does_not_flag_a_handler_that_invokes_a_restart() {
        assert!(
            violations("(handler-bind ((parse-error (lambda (c) (invoke-restart 'skip)))) (run))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_handler_that_returns_from_an_enclosing_block() {
        assert!(
            violations("(handler-bind ((parse-error (lambda (c) (return-from run nil)))) (run))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_handler_that_resignals() {
        assert!(
            violations("(handler-bind ((parse-error (lambda (c) (error c)))) (run))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_handler_that_only_has_a_side_effect() {
        assert!(
            violations("(handler-bind ((parse-error (lambda (c) (format t \"~A\" c)))) (run))")
                .is_empty(),
            "declining after logging is a real idiom"
        );
    }

    #[test]
    fn does_not_flag_a_named_handler_whose_body_is_elsewhere() {
        assert!(violations("(handler-bind ((parse-error #'note-it)) (run))").is_empty());
        assert!(violations("(handler-bind ((parse-error handler)) (run))").is_empty());
    }

    #[test]
    fn does_not_flag_an_empty_handler_body() {
        assert!(violations("(handler-bind ((parse-error (lambda (c)))) (run))").is_empty());
    }

    #[test]
    fn does_not_flag_handler_case_which_uses_its_clause_value() {
        assert!(violations("(handler-case (run) (parse-error (c) nil))").is_empty());
    }

    /// `#+sbcl nil` would otherwise read as a handler ending in `nil`, which is
    /// the exact shape this rule flags — so the guard has to be what keeps it
    /// quiet, not the absence of a match.
    #[test]
    fn does_not_flag_a_reader_conditional_tail_whose_arity_is_build_dependent() {
        assert_eq!(
            violations("(handler-bind ((parse-error (lambda (c) nil))) (run))").len(),
            1,
            "the same tail without the reader conditional is flagged"
        );
        assert!(
            violations("(handler-bind ((parse-error (lambda (c) #+sbcl nil))) (run))").is_empty()
        );
    }

    #[test]
    fn a_matching_shape_inside_a_quote_is_data_and_is_not_flagged() {
        assert!(violations("'(handler-bind ((parse-error (lambda (c) nil))) (run))").is_empty());
        assert!(
            violations("(quote (handler-bind ((parse-error (lambda (c) nil))) (run)))").is_empty()
        );
    }

    #[test]
    fn a_matching_shape_inside_a_backquote_with_no_unquote_is_data() {
        assert!(violations("`(handler-bind ((parse-error (lambda (c) nil))) (run))").is_empty());
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(
            violations("`(progn ,(handler-bind ((parse-error (lambda (c) nil))) (run)))").len(),
            1
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert!(
            violations("(format t \"(handler-bind ((parse-error (lambda (c) nil))) (run))\")")
                .is_empty()
        );
    }

    #[test]
    fn the_summary_counts_every_handler_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(handler-bind ((parse-error (lambda (c) nil))\n                 \
             (file-error (lambda (c) (invoke-restart 'skip))))\n  (run))",
        );
        assert_eq!(report.summary, vec![("handler_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn the_finding_carries_its_line_and_its_condition_type() {
        let report = report("(handler-bind\n    ((parse-error (lambda (c) nil)))\n  (run))");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "handler-bind-handler-returns-bare-value");
        assert_eq!(
            finding.json_fields(),
            vec![("condition", json!("parse-error"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["condition=parse-error".to_owned()]
        );
        assert!(finding.message().contains("declines to handle"));
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(handler-bind ((parse-error (lambda (c) nil))) (run))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_handler_bind_handler_returns_bare_value_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("handler_count", json!(0))]);
    }
}
