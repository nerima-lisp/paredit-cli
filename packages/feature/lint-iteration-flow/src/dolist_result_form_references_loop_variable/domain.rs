//! Common Lisp `dolist` result-form detection: a result form that reads the
//! loop variable.
//!
//! `(dolist (var list-form [result-form]) …)` evaluates `result-form` once,
//! after the list is exhausted — and CLHS is explicit that at that point `var`
//! is bound to **`nil`**. So `(dolist (x items x))` always returns `nil`, no
//! matter what `items` held, and `(dolist (x items (list x)))` always returns
//! `(nil)`.
//!
//! The mistake is natural: the result form is the idiomatic place to read the
//! accumulator a body has been building, and `var` looks like it should still
//! hold the last element.
//!
//! ```text
//! (dolist (x items (car x)) (process x))   ; (car nil), always
//! ```
//!
//! # What this rule recognises
//!
//! A `dolist` whose spec is exactly `(var list-form result-form)`, where `var`
//! is a plain symbol and `result-form` contains an evaluated,
//! non-operator-position reference to it.
//!
//! # What this rule deliberately does not flag
//!
//! - **`dotimes`.** Its result form is evaluated with the counter bound to the
//!   *limit*, which is a documented and useful value, not `nil`.
//! - **A result form that might rebind the name** — `(dolist (x items (let
//!   ((x 1)) x)))` reads its own `x`. See [`crate::support::may_rebind`].
//! - **A name in operator position**, and anything inside quoted or
//!   quasiquoted data.
//! - **Other dialects.** Emacs Lisp's `dolist` differs here, and this package
//!   is Common Lisp only.
//!
//! Note that `paredit-core-semantics`' binder resolves a `dolist` result form
//! *inside* the loop scope, which is right for name resolution — the reference
//! does resolve to that binding — and says nothing about the value it holds,
//! which is what this rule is about.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};
use serde_json::{Value, json};

use crate::support::{
    for_each_evaluated_subview, may_rebind, plain_variable_name, references_variable,
};

#[derive(Debug, Clone)]
pub struct DolistResultVariableItem {
    /// The span of the result form.
    pub span: ByteSpan,
    /// The loop variable the result form reads, lowercased.
    pub variable: String,
    /// The result form, re-rendered from the tree.
    pub result_form: String,
}

impl Finding for DolistResultVariableItem {
    fn kind(&self) -> &'static str {
        "dolist-result-form-references-loop-variable"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("variable={}", self.variable),
            format!("result={}", self.result_form),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable", json!(self.variable)),
            ("result_form", json!(self.result_form)),
        ]
    }

    /// The same sentence the `dolist-result-form-references-loop-variable`
    /// lint rule writes, so a SARIF or JUnit consumer reading both sees one
    /// finding described one way.
    fn message(&self) -> String {
        format!(
            "dolist result form {} reads `{}`, which the standard binds to nil once the \
             list is exhausted",
            self.result_form, self.variable
        )
    }
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_dolist_result(
    view: &ExpressionView,
    dolist_form_count: &mut usize,
    violations: &mut Vec<DolistResultVariableItem>,
) {
    if !is_paren_list(view) || !view.reader_prefixes.is_empty() {
        return;
    }
    if !list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case("dolist")) {
        return;
    }
    let Some(spec) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(spec) {
        return;
    }
    *dolist_form_count += 1;

    // Only the three-element spec has a result form at all.
    if spec.children.len() != 3 {
        return;
    }
    let Some(variable) = spec.children.first().and_then(plain_variable_name) else {
        return;
    };
    let Some(result) = spec.children.get(2) else {
        return;
    };

    if may_rebind(result, &variable) {
        return;
    }
    if references_variable(result, &variable) {
        violations.push(DolistResultVariableItem {
            span: result.span,
            variable,
            result_form: render_expression(result),
        });
    }
}

/// Collects every `dolist` result form that reads its loop variable in one
/// file, with the number of `dolist` forms scanned as the denominator beside
/// them.
///
/// A dialect this rule does not model is reported as unmodelled rather than as
/// clean: an empty finding list means "no result form reads the variable" for
/// Common Lisp and "nothing was looked for" for Clojure, and the two read
/// identically without the flag.
pub fn build_dolist_result_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DolistResultVariableItem>> {
    let mut dolist_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_dolist_result(subview, &mut dolist_form_count, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("dolist_form_count", json!(dolist_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DolistResultVariableItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_dolist_result_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build dolist result report")
    }

    fn violations(input: &str) -> Vec<DolistResultVariableItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_result_form_that_is_the_loop_variable() {
        let found = violations("(dolist (x items x) (process x))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].variable, "x");
        assert_eq!(found[0].result_form, "x");
    }

    #[test]
    fn flags_a_result_form_that_reads_the_loop_variable() {
        let found = violations("(dolist (x items (car x)) (process x))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].result_form, "(car x)");
    }

    #[test]
    fn does_not_flag_a_result_form_that_reads_an_accumulator() {
        assert!(violations("(dolist (x items acc) (push x acc))").is_empty());
    }

    #[test]
    fn does_not_flag_a_spec_with_no_result_form() {
        assert!(violations("(dolist (x items) (process x))").is_empty());
    }

    #[test]
    fn does_not_flag_dotimes() {
        // `dotimes` binds its counter to the limit in the result form.
        assert!(violations("(dotimes (i n i) (process i))").is_empty());
    }

    #[test]
    fn does_not_flag_a_name_in_operator_position() {
        assert!(violations("(dolist (f fns (f 1)) (call f))").is_empty());
    }

    #[test]
    fn does_not_flag_a_result_form_that_rebinds_the_name() {
        assert!(violations("(dolist (x items (let ((x 1)) x)) (process x))").is_empty());
    }

    /// The quote must be what suppresses this, so the input puts `x` in an
    /// *argument* position: `(f x)` unquoted is a finding, and the only
    /// difference between the two assertions is the quote. Written as `'(x)`
    /// the test would prove nothing, because `references_variable` skips a
    /// paren list's operator whether the list is quoted or not.
    #[test]
    fn does_not_flag_a_reference_inside_quoted_data() {
        assert_eq!(violations("(dolist (x items (f x)) (process x))").len(), 1);
        assert!(violations("(dolist (x items '(f x)) (process x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reference_inside_a_quasiquoted_template() {
        assert!(violations("(dolist (x items `(f x)) (process x))").is_empty());
    }

    #[test]
    fn does_not_flag_the_name_inside_a_string_literal() {
        assert!(violations("(dolist (x items \"x\") (process x))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_dolist() {
        assert!(violations("(defparameter *f* '(dolist (x items x) (process x)))").is_empty());
    }

    /// The quote need not sit on the `dolist` itself: a target nested *inside*
    /// quoted data is still data.
    #[test]
    fn does_not_flag_a_dolist_nested_inside_quoted_data() {
        assert!(
            violations("(defparameter *c* '(progn (dolist (x items x) (process x))))").is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_dolist_nested_inside_a_macro_template() {
        assert!(
            violations(
                "(defmacro m (&body body)\n  `(progn (dolist (x items x) (process x)) ,@body))"
            )
            .is_empty()
        );
    }

    #[test]
    fn finds_a_result_reference_nested_in_a_function_body() {
        let found = violations("(defun f (items)\n  (dolist (x items (car x))\n    (process x)))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree =
            SyntaxTree::parse_with_dialect("(dolist (x items x) (process x))", Dialect::Clojure)
                .expect("parse");
        let report = build_dolist_result_report(Path::new("app.clj"), Dialect::Clojure, &tree)
            .expect("build dolist result report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("dolist_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(dolist (x items) (process x))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(defun f (items)\n  (dolist (x items (car x)) (process x)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(
            finding.kind(),
            "dolist-result-form-references-loop-variable"
        );
        assert_eq!(
            finding.json_fields(),
            vec![("variable", json!("x")), ("result_form", json!("(car x)"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["variable=x".to_owned(), "result=(car x)".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "dolist result form (car x) reads `x`, which the standard binds to nil once \
             the list is exhausted"
        );
    }

    #[test]
    fn the_summary_counts_every_dolist_scanned_not_only_the_flagged_ones() {
        let report = report("(dolist (x items x) (f x))\n(dolist (y items acc) (g y))\n");
        assert_eq!(report.summary, vec![("dolist_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
