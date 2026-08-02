//! Common Lisp all-unused-`multiple-value-bind` detection: a
//! `(multiple-value-bind (v…) form body…)` whose body mentions none of the
//! variables it binds.
//!
//! `multiple-value-bind` exists to *name* the extra values a form returns
//! (CLHS `multiple-value-bind`). A body that never mentions any of them is
//! doing what `(progn form body…)` does, one binding form deeper, and the
//! reader has to check every variable to find that out.
//!
//! # What stops a finding
//!
//! - **A `(declare …)` naming any bound variable.** `(ignore …)` and
//!   `(ignorable …)` are the cases this exists for: that is the *correct*
//!   spelling for "this value is deliberately dropped" — CLHS requires it
//!   precisely so the compiler stops warning — and flagging it would tell an
//!   author to undo the right thing. Any declaration over any of the variables
//!   silences the whole form.
//! - **Any occurrence of a bound name anywhere in the body's code**, in any
//!   position,
//!   quoted or not, at any depth. `(list 'x)` counts as mentioning `x`. That
//!   over-counts on purpose: a macro may build a reference out of a quoted
//!   symbol, and a missed finding is cheaper than a wrong one.
//! - Anything but a plain symbol in the variable list, an empty variable list,
//!   or an empty body — all of which are somebody else's subject.
//!
//! The value-producing form is never searched: the variables are not in scope
//! there.
//!
//! # Relationship to `single-value-bind`
//!
//! `paredit-feature-lint-form-shape`'s `single-value-bind` matches the same
//! head and asks a disjoint question — whether the variable list holds exactly
//! one name, which a `let` would say better. A one-variable form whose body
//! ignores that variable earns both findings, which are two true statements
//! about it.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{for_each_subview, is_paren_list, list_head, symbol_is};
use serde_json::{Value, json};

use crate::support::{is_unevaluated_at, normalized_symbol, plain_name};

#[derive(Debug, Clone)]
pub struct MultipleValueBindAllIgnoredItem {
    /// The span of the whole `(multiple-value-bind …)` form.
    pub span: ByteSpan,
    /// The variables it binds, normalized, in source order.
    pub variables: Vec<String>,
}

impl Finding for MultipleValueBindAllIgnoredItem {
    fn kind(&self) -> &'static str {
        "multiple-value-bind-all-ignored"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!("variables={}", self.variables.join(","))]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![("variables", json!(self.variables))]
    }

    fn message(&self) -> String {
        message_for(&self.variables)
    }
}

/// The one sentence both the report and the lint rule phrase a finding with.
#[must_use]
pub fn message_for(variables: &[String]) -> String {
    format!(
        "multiple-value-bind binds {} that the body never references; \
         it is a progn with extra steps, or the variables want (declare (ignore …))",
        variables.join(", ")
    )
}

/// Whether any `(declare …)` in the body names one of the bound variables.
///
/// `(ignore …)` and `(ignorable …)` are the cases this exists for — they are
/// the *correct* spelling of a deliberately dropped value, and reporting them
/// would tell an author to undo the right thing. Every other declaration kind
/// is treated identically on purpose: `(declare (type fixnum q))` is the
/// author talking about `q`, and this rule's subject is a body that says
/// nothing about the variables at all.
///
/// Reads every `(declare …)` in the body rather than only the leading run:
/// which position a declaration is legal in is a different rule's subject.
fn declaration_names_any(body: &[ExpressionView], variables: &[String]) -> bool {
    let mut named = false;
    for declare in body
        .iter()
        .filter(|form| list_head(form).is_some_and(|head| symbol_is(head, "declare")))
    {
        for_each_subview(declare, |subview| {
            if !named
                && normalized_symbol(subview)
                    .as_deref()
                    .is_some_and(|name| variables.iter().any(|variable| variable == name))
            {
                named = true;
            }
        });
        if named {
            return true;
        }
    }
    false
}

/// Whether any of `variables` is mentioned anywhere in the body's *code*.
///
/// Declarations are excluded and answered by [`declaration_names_any`]
/// instead. Folding the two together would work by accident — a
/// `(declare (ignore q))` does contain the token `q` — and would silently
/// stop protecting the `ignore` idiom the day someone decided, quite
/// reasonably, that a declaration is not a reference.
fn is_declaration(form: &ExpressionView) -> bool {
    list_head(form).is_some_and(|head| symbol_is(head, "declare"))
}

fn mentions_any(body: &[ExpressionView], variables: &[String]) -> bool {
    let mut mentioned = false;
    for form in body.iter().filter(|form| !is_declaration(form)) {
        for_each_subview(form, |subview| {
            if mentioned {
                return;
            }
            if let Some(name) = normalized_symbol(subview) {
                mentioned = variables.contains(&name);
            }
        });
        if mentioned {
            return true;
        }
    }
    false
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
///
/// Reads only the matched form's own subtree.
pub fn examine_multiple_value_bind(
    tree: &SyntaxTree,
    view: &ExpressionView,
    multiple_value_bind_form_count: &mut usize,
    violations: &mut Vec<MultipleValueBindAllIgnoredItem>,
) {
    if !is_paren_list(view)
        || !list_head(view).is_some_and(|head| symbol_is(head, "multiple-value-bind"))
    {
        return;
    }
    *multiple_value_bind_form_count += 1;

    // `(multiple-value-bind (var*) values-form declaration* form*)`. A form
    // with no body at all binds nothing anybody could reference, and is a
    // shape question rather than a usage one.
    let Some(variable_list) = view.children.get(1) else {
        return;
    };
    if !is_paren_list(variable_list) || variable_list.children.is_empty() {
        return;
    }
    let body = &view.children[3.min(view.children.len())..];
    if body.is_empty() {
        return;
    }

    // Every entry must be a plain symbol; anything else means this is not the
    // shape being read, and guessing at it would put a wrong name in a report.
    let mut variables = Vec::with_capacity(variable_list.children.len());
    for entry in &variable_list.children {
        let Some(name) = plain_name(entry) else {
            return;
        };
        variables.push(name);
    }

    if declaration_names_any(body, &variables) || mentions_any(body, &variables) {
        return;
    }
    if is_unevaluated_at(tree, view.span) {
        return;
    }

    violations.push(MultipleValueBindAllIgnoredItem {
        span: view.span,
        variables,
    });
}

/// Collects every all-unused `multiple-value-bind` in one file, with the number
/// of `multiple-value-bind` forms scanned as the denominator beside them.
pub fn build_multiple_value_bind_all_ignored_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MultipleValueBindAllIgnoredItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("multiple_value_bind_form_count", json!(0))],
        ));
    }

    let mut multiple_value_bind_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_multiple_value_bind(
                tree,
                subview,
                &mut multiple_value_bind_form_count,
                &mut violations,
            );
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![(
            "multiple_value_bind_form_count",
            json!(multiple_value_bind_form_count),
        )],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(input: &str) -> FileFindings<MultipleValueBindAllIgnoredItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_multiple_value_bind_all_ignored_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn variables(input: &str) -> Vec<Vec<String>> {
        report(input)
            .findings
            .into_iter()
            .map(|item| item.variables)
            .collect()
    }

    // -- positive -----------------------------------------------------------

    #[test]
    fn flags_a_body_that_references_no_bound_variable() {
        assert_eq!(
            variables("(multiple-value-bind (q r) (floor 7 2) (print \"done\"))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    #[test]
    fn flags_a_single_variable_nobody_reads() {
        assert_eq!(
            variables("(multiple-value-bind (value) (gethash k h) (compute))"),
            vec![vec!["value".to_owned()]]
        );
    }

    #[test]
    fn flags_a_body_that_mentions_a_similar_but_different_name() {
        assert_eq!(
            variables("(multiple-value-bind (q r) (floor 7 2) (print quotient))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    // -- near-miss negatives ------------------------------------------------

    #[test]
    fn does_not_flag_a_body_that_references_one_of_the_variables() {
        assert!(variables("(multiple-value-bind (q r) (floor 7 2) (print q))").is_empty());
    }

    #[test]
    fn does_not_flag_a_reference_nested_deep_in_the_body() {
        assert!(
            variables("(multiple-value-bind (q r) (floor 7 2) (when t (let ((x 1)) (+ x r))))")
                .is_empty()
        );
    }

    /// `(declare (ignore …))` is the correct spelling of a dropped value, not
    /// a defect to report.
    #[test]
    fn does_not_flag_a_form_that_declares_its_variables_ignored() {
        assert!(
            variables("(multiple-value-bind (q r) (floor 7 2) (declare (ignore q r)) (print 1))")
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_form_that_declares_its_variables_ignorable() {
        assert!(
            variables(
                "(multiple-value-bind (q r) (floor 7 2) (declare (ignorable q r)) (print 1))"
            )
            .is_empty()
        );
    }

    /// One `ignore` is enough: the author has already said the values are
    /// deliberate.
    #[test]
    fn does_not_flag_a_form_that_declares_only_one_variable_ignored() {
        assert!(
            variables("(multiple-value-bind (q r) (floor 7 2) (declare (ignore q)) (print 1))")
                .is_empty()
        );
    }

    /// Any declaration naming a variable counts, not only `ignore`: an author
    /// who wrote a type for `q` is talking about `q`.
    #[test]
    fn does_not_flag_a_form_whose_declaration_names_a_variable_for_another_reason() {
        assert!(
            variables(
                "(multiple-value-bind (q r) (floor 7 2) (declare (type fixnum q)) (print 1))"
            )
            .is_empty()
        );
    }

    /// The declaration is answered by its own guard, not by the reference
    /// scan: a `(declare (ignore q))` is not a *use* of `q`.
    #[test]
    fn a_declaration_is_not_counted_as_a_reference() {
        assert!(!mentions_any(
            &[
                SyntaxTree::parse_with_dialect("(declare (ignore q))", Dialect::CommonLisp)
                    .expect("parse")
                    .root_view()
                    .children
                    .remove(0)
            ],
            &["q".to_owned()]
        ));
    }

    #[test]
    fn does_not_flag_a_form_with_no_body() {
        assert!(variables("(multiple-value-bind (q r) (floor 7 2))").is_empty());
    }

    #[test]
    fn does_not_flag_a_form_with_an_empty_variable_list() {
        assert!(variables("(multiple-value-bind () (floor 7 2) (print 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_malformed_variable_list() {
        assert!(variables("(multiple-value-bind q (floor 7 2) (print 1))").is_empty());
        assert!(variables("(multiple-value-bind ((q)) (floor 7 2) (print 1))").is_empty());
    }

    /// The variables are not in scope in the value-producing form, so a
    /// mention there is not a reference.
    #[test]
    fn does_not_read_the_values_form_as_a_reference() {
        assert_eq!(
            variables("(multiple-value-bind (q r) (floor q r) (print 1))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    /// A quoted mention counts, deliberately: a macro may turn it into one.
    #[test]
    fn does_not_flag_a_quoted_mention_in_the_body() {
        assert!(variables("(multiple-value-bind (q r) (floor 7 2) (my-macro 'q))").is_empty());
    }

    #[test]
    fn case_folds_and_ignores_the_package_qualifier() {
        assert!(variables("(MULTIPLE-VALUE-BIND (Q R) (floor 7 2) (print Q))").is_empty());
        assert_eq!(
            variables("(CL:MULTIPLE-VALUE-BIND (Q R) (floor 7 2) (print 1))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    // -- the five quote shapes ---------------------------------------------

    #[test]
    fn does_not_flag_a_hard_quoted_form() {
        assert!(variables("'(multiple-value-bind (q r) (floor 7 2) (print 1))").is_empty());
    }

    #[test]
    fn does_not_flag_a_long_hand_quote_form() {
        assert!(variables("(quote (multiple-value-bind (q r) (floor 7 2) (print 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_comma_inside_a_hard_quote() {
        assert!(variables("'(a ,(multiple-value-bind (q r) (floor 7 2) (print 1)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_quasiquoted_macro_template() {
        assert!(
            variables("(defmacro m () `(multiple-value-bind (q r) (floor 7 2) (print 1)))")
                .is_empty()
        );
    }

    #[test]
    fn flags_an_unquoted_form_inside_a_quasiquote() {
        assert_eq!(
            variables("(defmacro m () `(a ,(multiple-value-bind (q r) (floor 7 2) (print 1))))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    // -- strings ------------------------------------------------------------

    #[test]
    fn does_not_read_a_variable_name_inside_a_string_literal_as_a_reference() {
        assert_eq!(
            variables("(multiple-value-bind (q r) (floor 7 2) (print \"q and r\"))"),
            vec![vec!["q".to_owned(), "r".to_owned()]]
        );
    }

    // -- report shape -------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(multiple-value-bind (q r) (floor 7 2) (print 1))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_multiple_value_bind_all_ignored_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
        assert_eq!(
            report.summary,
            vec![("multiple_value_bind_form_count", json!(0))]
        );
    }

    #[test]
    fn the_summary_counts_every_form_scanned_not_only_the_flagged_ones() {
        let report = report(
            "(multiple-value-bind (q r) (floor 7 2) (print q))\n\
             (multiple-value-bind (a b) (floor 7 2) (print 1))\n",
        );
        assert_eq!(
            report.summary,
            vec![("multiple_value_bind_form_count", json!(2))]
        );
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_and_its_variables() {
        let report = report("(defun f ()\n  (multiple-value-bind (q r) (floor 7 2) (print 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "multiple-value-bind-all-ignored");
        assert_eq!(
            finding.json_fields(),
            vec![("variables", json!(["q", "r"]))]
        );
        assert_eq!(finding.text_columns(), vec!["variables=q,r".to_owned()]);
    }
}
