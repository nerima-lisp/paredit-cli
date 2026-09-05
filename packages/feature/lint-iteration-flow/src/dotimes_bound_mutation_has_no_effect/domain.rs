//! Common Lisp `dotimes` bound-mutation detection: assigning the variable
//! that gave `dotimes` its iteration count, from inside the body.
//!
//! `(dotimes (i n) …)` evaluates `n` **once**, on entry, and iterates that
//! many times. A body that does `(setq n 0)` or `(incf n)` is written by
//! someone reading `dotimes` as a C `for (i = 0; i < n; i++)`, where the test
//! re-reads `n` every pass. In Common Lisp it does not: the loop runs the
//! original number of times regardless.
//!
//! ```text
//! (dotimes (i limit)
//!   (when (done-p i) (setq limit 0))   ; does not shorten the loop
//!   (step i))
//! ```
//!
//! # What this rule recognises
//!
//! A `dotimes` whose count form is a **bare variable name**, whose body
//! assigns that exact name with `setq`, `psetq`, `setf`, `psetf`, `incf`, or
//! `decf`. The assignment must name the variable directly as a place; a
//! `(setf (aref n 0) …)` is a different place and is not flagged.
//!
//! # What this rule deliberately does not flag
//!
//! - **A computed count form.** `(dotimes (i (length xs)) … (push y xs))` also
//!   fails to lengthen the loop, but "does the body mutate something this
//!   expression read?" is a much larger question than this rule asks, and
//!   getting it wrong means warning about correct code.
//! - **Any `dotimes` whose body might rebind the name.** If a `let`, `lambda`,
//!   inner `dotimes`, or `loop` anywhere in the body so much as mentions the
//!   name in a binding position, the whole form is skipped: the assignment
//!   might be to a different variable that merely shares a spelling. See
//!   [`crate::support::may_rebind`], which deliberately answers "might".
//! - **Anything inside quoted or quasiquoted data**, which is not this call's
//!   code. This holds at both levels: a `dotimes` written inside a quoted list
//!   or a macro template is never examined at all, and an assignment written
//!   inside quoted data within an examined body is not a finding. Both use
//!   [`crate::support::for_each_evaluated_subview`], which stops at a quote.
//!
//!   One consequence is worth stating plainly rather than leaving implied: an
//!   *unquoted* sub-form inside a template — the `,(setq n 0)` of
//!   `` `(dotimes (i n) ,(setq n 0)) `` — really is code, and this rule does
//!   not see it either, because the walk stops at the enclosing quasiquote
//!   without descending to find the unquote. That is a false negative, which
//!   is the direction this package errs in deliberately.
//! - **`dolist` and `do`.** `dolist`'s list form is likewise evaluated once,
//!   but mutating a list *variable* is a normal idiom (the loop still walks
//!   the original cells), so the same reasoning does not carry over.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, unqualified};
use serde_json::{Value, json};

use crate::support::{for_each_evaluated_subview, may_rebind, names_variable, plain_variable_name};

/// Operators whose *odd-indexed* arguments are places.
const PAIRWISE_ASSIGNMENTS: [&str; 4] = ["setq", "psetq", "setf", "psetf"];

/// Operators whose single place is their first argument.
const SINGLE_PLACE_ASSIGNMENTS: [&str; 2] = ["incf", "decf"];

#[derive(Debug, Clone)]
pub struct DotimesBoundMutationItem {
    /// The span of the assignment form.
    pub span: ByteSpan,
    /// The count variable being assigned, lowercased.
    pub bound_variable: String,
    /// The assignment operator, lowercased.
    pub operator: String,
}

impl Finding for DotimesBoundMutationItem {
    fn kind(&self) -> &'static str {
        "dotimes-bound-mutation-has-no-effect"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![
            format!("variable={}", self.bound_variable),
            format!("operator={}", self.operator),
        ]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("bound_variable", json!(self.bound_variable)),
            ("operator", json!(self.operator)),
        ]
    }

    fn message(&self) -> String {
        format!(
            "`{}` assigns `{}`, which dotimes evaluated once on entry; \
             this does not change the number of iterations",
            self.operator, self.bound_variable
        )
    }
}

/// The assignment operator of a form that assigns `name` as a place, or
/// `None`.
fn assignment_of(view: &ExpressionView, name: &str) -> Option<String> {
    if !view.reader_prefixes.is_empty() {
        return None;
    }
    let head = unqualified(list_head(view)?).to_ascii_lowercase();
    let assigns = if PAIRWISE_ASSIGNMENTS.contains(&head.as_str()) {
        view.children
            .iter()
            .skip(1)
            .step_by(2)
            .any(|place| names_variable(place, name))
    } else if SINGLE_PLACE_ASSIGNMENTS.contains(&head.as_str()) {
        view.children
            .get(1)
            .is_some_and(|place| names_variable(place, name))
    } else {
        false
    };
    assigns.then_some(head)
}

pub fn examine_dotimes_bound_mutation(
    view: &ExpressionView,
    dotimes_form_count: &mut usize,
    violations: &mut Vec<DotimesBoundMutationItem>,
) {
    if !is_paren_list(view) || !view.reader_prefixes.is_empty() {
        return;
    }
    if !list_head(view).is_some_and(|head| unqualified(head).eq_ignore_ascii_case("dotimes")) {
        return;
    }
    let Some(spec) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(spec) {
        return;
    }
    *dotimes_form_count += 1;

    let Some(count_form) = spec.children.get(1) else {
        return;
    };
    // Only a bare variable count form is modelled; see the module doc.
    let Some(bound) = plain_variable_name(count_form) else {
        return;
    };
    // `(dotimes (n n) …)` counts with its own loop variable; assigning it is a
    // different mistake and not this rule's.
    if spec
        .children
        .first()
        .and_then(plain_variable_name)
        .is_some_and(|variable| variable == bound)
    {
        return;
    }

    let body = &view.children[2.min(view.children.len())..];
    // Any possible rebinding makes the name ambiguous; say nothing.
    if body.iter().any(|form| may_rebind(form, &bound)) {
        return;
    }

    for form in body {
        for_each_evaluated_subview(form, |subview| {
            if let Some(operator) = assignment_of(subview, &bound) {
                violations.push(DotimesBoundMutationItem {
                    span: subview.span,
                    bound_variable: bound.clone(),
                    operator,
                });
            }
        });
    }
}

/// Collects every ineffective `dotimes` bound mutation in one file, with the
/// number of `dotimes` forms scanned as the denominator beside them.
///
/// Reports unsupported dialects as unmodelled.
pub fn build_dotimes_bound_mutation_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DotimesBoundMutationItem>> {
    let mut dotimes_form_count = 0;
    let mut violations = Vec::new();

    if dialect == Dialect::CommonLisp {
        for index in 0..tree.root_children().len() {
            let view = tree.select_path(&SexprPath::root_child(index))?.view();
            for_each_evaluated_subview(&view, |subview| {
                examine_dotimes_bound_mutation(subview, &mut dotimes_form_count, &mut violations);
            });
        }
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        dialect == Dialect::CommonLisp,
        tree.source(),
        violations,
        vec![("dotimes_form_count", json!(dotimes_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<DotimesBoundMutationItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_dotimes_bound_mutation_report(Path::new("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("build dotimes bound mutation report")
    }

    fn violations(input: &str) -> Vec<DotimesBoundMutationItem> {
        report(input).findings
    }

    #[test]
    fn flags_a_setq_of_the_count_variable() {
        let found = violations("(dotimes (i n) (setq n 0))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bound_variable, "n");
        assert_eq!(found[0].operator, "setq");
    }

    #[test]
    fn flags_a_setf_of_the_count_variable() {
        let found = violations("(dotimes (i limit) (when (done-p i) (setf limit 0)) (step i))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].operator, "setf");
    }

    #[test]
    fn flags_incf_and_decf_of_the_count_variable() {
        assert_eq!(violations("(dotimes (i n) (incf n))").len(), 1);
        assert_eq!(violations("(dotimes (i n) (decf n 2))").len(), 1);
    }

    #[test]
    fn flags_a_setf_that_assigns_the_count_variable_among_several_places() {
        let found = violations("(dotimes (i n) (setf a 1 n 0 b 2))");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bound_variable, "n");
    }

    #[test]
    fn does_not_flag_a_body_that_only_reads_the_count_variable() {
        assert!(violations("(dotimes (i n) (print (- n i)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_setf_of_a_different_place() {
        assert!(violations("(dotimes (i n) (setf (aref n i) 0))").is_empty());
        assert!(violations("(dotimes (i n) (setf total 0))").is_empty());
    }

    #[test]
    fn does_not_flag_an_assignment_of_the_loop_variable() {
        assert!(violations("(dotimes (i n) (setq i 0))").is_empty());
    }

    #[test]
    fn does_not_flag_a_computed_count_form() {
        assert!(violations("(dotimes (i (length xs)) (setq xs nil))").is_empty());
    }

    /// A body that might rebind the name makes the assignment ambiguous.
    #[test]
    fn does_not_flag_when_the_body_may_rebind_the_name() {
        assert!(violations("(dotimes (i n) (let ((n 0)) (setq n 1)))").is_empty());
        assert!(violations("(dotimes (i n) (loop for n from 1 to 3 do (setq n 2)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_assignment_in_quoted_data() {
        assert!(violations("(dotimes (i n) (eval '(setq n 0)))").is_empty());
    }

    #[test]
    fn does_not_flag_an_assignment_in_a_quasiquoted_template() {
        assert!(violations("(dotimes (i n) (push `(setq n 0) forms))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quoted_dotimes() {
        assert!(violations("(defparameter *f* '(dotimes (i n) (setq n 0)))").is_empty());
    }

    /// The quote need not sit on the `dotimes` itself: a target nested
    /// *inside* quoted data is still data.
    #[test]
    fn does_not_flag_a_dotimes_nested_inside_quoted_data() {
        assert!(violations("(defparameter *b* '(list (dotimes (i n) (setq n 0))))").is_empty());
    }

    /// The realistic bite: a macro template. The `dotimes` written here is the
    /// expansion's code, and `limit` there is the *expansion's* variable.
    #[test]
    fn does_not_flag_a_dotimes_nested_inside_a_macro_template() {
        assert!(
            violations(
                "(defmacro repeat-until (limit &body body)\n  \
                 `(progn (dotimes (i limit) (setq limit 0)) ,@body))"
            )
            .is_empty()
        );
    }

    #[test]
    fn does_not_flag_the_name_inside_a_string_literal() {
        assert!(violations("(dotimes (i n) (print \"(setq n 0)\"))").is_empty());
    }

    #[test]
    fn does_not_flag_a_dolist() {
        assert!(violations("(dolist (x items) (setq items nil))").is_empty());
    }

    #[test]
    fn finds_a_mutation_nested_in_a_function_body() {
        let found = violations("(defun f (n)\n  (dotimes (i n)\n    (setq n 0)))");
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect("(dotimes (i n) (setq n 0))", Dialect::Clojure)
            .expect("parse");
        let report =
            build_dotimes_bound_mutation_report(Path::new("app.clj"), Dialect::Clojure, &tree)
                .expect("build dotimes bound mutation report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, vec![("dotimes_form_count", json!(0))]);
    }

    #[test]
    fn a_common_lisp_file_is_reported_as_modelled() {
        assert!(report("(dotimes (i 10) (print i))").dialect_modelled);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_fields() {
        let report = report("(defun f (n)\n  (dotimes (i n) (setq n 0)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "dotimes-bound-mutation-has-no-effect");
        assert_eq!(
            finding.json_fields(),
            vec![("bound_variable", json!("n")), ("operator", json!("setq"))]
        );
        assert_eq!(
            finding.text_columns(),
            vec!["variable=n".to_owned(), "operator=setq".to_owned()]
        );
        assert_eq!(
            finding.message(),
            "`setq` assigns `n`, which dotimes evaluated once on entry; \
             this does not change the number of iterations"
        );
    }

    #[test]
    fn the_summary_counts_every_dotimes_scanned_not_only_the_flagged_ones() {
        let report = report("(dotimes (i n) (setq n 0))\n(dotimes (j m) (print j))\n");
        assert_eq!(report.summary, vec![("dotimes_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }
}
