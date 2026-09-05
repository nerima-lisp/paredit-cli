//! Common Lisp unused-`&whole` detection: a `destructuring-bind` whose
//! `&whole` variable is never referenced anywhere in the form.
//!
//! `(destructuring-bind (&whole all p q) expr body…)` binds `all` to the whole
//! list `expr` produced, alongside the destructured `p` and `q` — verified
//! against SBCL, which evaluates the form above with `'(1 2)` to
//! `((1 2) 1 2)`. A `&whole` nobody reads is a binding whose value is
//! computed and discarded: dead code, and usually a leftover from an edit that
//! removed the last reference.
//!
//! # Scope: `destructuring-bind` only, and why
//!
//! `&whole` is also legal in a *macro* lambda list (`defmacro`,
//! `define-compiler-macro`), and this rule deliberately does **not** look
//! there, because `inspect unused-parameters` already does. Verified by running
//! the shipped binary on
//! `(defmacro m (&whole w a b) (list a b))`, which it reports as
//! `{"head": "defmacro", "parameter_name": "w"}`. The same run says nothing
//! about `(destructuring-bind (&whole all p q) x (list p q))`, because
//! `destructuring-bind` is not a *definition* form and that report only reads
//! definitions. Covering the macro case here would ship a near-copy of an
//! existing detector; covering `destructuring-bind` is the part nothing else
//! reaches.
//!
//! # What counts as a reference
//!
//! Any atom naming the variable, anywhere under the `destructuring-bind` form,
//! **including inside quoted or backquoted data**. That is deliberately
//! over-generous:
//!
//! - `` `(check ,all) `` is a real reference, and a walk that skipped data
//!   would call it unused. This is the trap the rule exists to avoid.
//! - `(destructuring-bind (&whole all p &optional (q (car all))) …)` references
//!   it from the lambda list's own init form, which is why the search covers
//!   the whole form and not only the body.
//! - `#'all` counts, since [`crate::support::normalized_symbol`] reads past the
//!   reader prefix.
//!
//! Over-counting can only make the rule quieter. A string literal never counts:
//! its atom text keeps the `"`, so a docstring mentioning the name is not a
//! reference.
//!
//! # What this rule deliberately does not flag
//!
//! - **A `&whole` that is not the lambda list's first element.** CLHS 3.4.4
//!   requires `&whole` to come first, so anything else is either malformed or a
//!   `&whole` belonging to a *nested* destructuring pattern with its own scope,
//!   which this reader does not model.
//! - **A variable named `_` or `_something`**, the cross-dialect spelling for
//!   "deliberately unused".
//! - **A `&whole` whose variable is not a plain symbol.**
//! - **A form reached only as quoted data.**
//!
//! Report only: deleting `&whole all` is mechanical, but a `&whole` is often
//! kept as documentation of the expected shape, and this project has a
//! documented history of autofixes silently removing live code.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{
    atom_is, bindable_variable_name, count_symbol_occurrences, for_each_evaluated_subview,
};

#[derive(Debug, Clone)]
pub struct DestructuringBindUnusedWholeItem {
    /// The span of the whole `(destructuring-bind …)` form.
    pub span: ByteSpan,
    /// The span of the unused variable itself, for an editor to jump to.
    pub variable_span: ByteSpan,
    /// The variable name, normalized.
    pub variable: String,
}

impl Finding for DestructuringBindUnusedWholeItem {
    fn kind(&self) -> &'static str {
        "destructuring-bind-unused-whole"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![self.variable.clone()]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable", json!(self.variable)),
            (
                "variable_span",
                json!({
                    "start": self.variable_span.start().get(),
                    "end": self.variable_span.end().get(),
                }),
            ),
        ]
    }

    fn message(&self) -> String {
        format!(
            "destructuring-bind binds &whole {} and never references it; drop the &whole",
            self.variable
        )
    }
}

/// The `&whole` variable a destructuring lambda list opens with, if it opens
/// with one that this reader can name.
fn leading_whole_variable(lambda_list: &ExpressionView) -> Option<(&ExpressionView, String)> {
    if !is_paren_list(lambda_list) || !lambda_list.reader_prefixes.is_empty() {
        return None;
    }
    // CLHS 3.4.4: `&whole` must be the first element of the lambda list. A
    // `&whole` anywhere else belongs to a nested pattern with its own scope,
    // which this reader does not model.
    if !lambda_list
        .children
        .first()
        .is_some_and(|first| atom_is(first, "&whole"))
    {
        return None;
    }
    let variable = lambda_list.children.get(1)?;
    bindable_variable_name(variable).map(|name| (variable, name))
}

/// Cheapest predicate first: the head comparison, then the form's own arity,
/// then the lambda list's first two elements. The subtree scan for references
/// happens last and only for a form that actually opens with a nameable
/// `&whole`, so a file of ordinary `destructuring-bind`s never pays for it.
pub fn examine(
    view: &ExpressionView,
    whole_binding_count: &mut usize,
    violations: &mut Vec<DestructuringBindUnusedWholeItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["destructuring-bind"]) {
        return;
    }
    // `(destructuring-bind lambda-list expression body…)`.
    if view.children.len() < 3 {
        return;
    }
    let Some((variable, name)) = leading_whole_variable(&view.children[1]) else {
        return;
    };
    *whole_binding_count += 1;

    // The binding occurrence itself is the one guaranteed hit; a second means
    // somebody reads it.
    if count_symbol_occurrences(view, &name) > 1 {
        return;
    }
    violations.push(DestructuringBindUnusedWholeItem {
        span: view.span,
        variable_span: variable.span,
        variable: name,
    });
}

/// Collects every `destructuring-bind` in one file whose `&whole` variable is
/// unreferenced, with the number of `&whole`-opening lambda lists scanned as
/// the denominator beside them.
pub fn build_destructuring_bind_unused_whole_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<DestructuringBindUnusedWholeItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("whole_binding_count", json!(0))],
        ));
    }

    let mut whole_binding_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut whole_binding_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("whole_binding_count", json!(whole_binding_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<DestructuringBindUnusedWholeItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_destructuring_bind_unused_whole_report(
            Path::new("test.lisp"),
            Dialect::CommonLisp,
            &tree,
        )
        .expect("build report")
    }

    fn fires(source: &str) -> bool {
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let root = tree.root_view();
        let mut found = false;
        paredit_core_syntax::view_query::for_each_subview(&root, |view| {
            let mut count = 0;
            let mut items = Vec::new();
            examine(view, &mut count, &mut items);
            if !items.is_empty() && !is_unevaluated_at(&tree, view.span) {
                found = true;
            }
        });
        found
    }

    // -- positive ------------------------------------------------------------

    #[test]
    fn flags_an_unreferenced_whole() {
        let violations = report("(destructuring-bind (&whole all p q) x (list p q))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].variable, "all");
    }

    #[test]
    fn the_variable_span_covers_the_name() {
        let source = "(destructuring-bind (&whole all p q) x (list p q))";
        let violations = report(source).findings;
        let span = violations[0].variable_span;
        assert_eq!(&source[span.start().get()..span.end().get()], "all");
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report("(CL:DESTRUCTURING-BIND (&WHOLE All p) x p)")
                .findings
                .len(),
            1
        );
    }

    // -- the reference traps --------------------------------------------------

    #[test]
    fn does_not_flag_a_whole_used_in_the_body() {
        assert!(
            report("(destructuring-bind (&whole all p q) x (list all p q))")
                .findings
                .is_empty()
        );
    }

    /// The trap the module doc names: a reference from inside a nested
    /// backquote template is a real reference.
    #[test]
    fn does_not_flag_a_whole_referenced_only_inside_a_backquote_template() {
        assert!(
            report("(destructuring-bind (&whole all p q) x `(check ,all ,p ,q))")
                .findings
                .is_empty()
        );
    }

    /// And a reference inside a *hard* quote is counted too — over-generous on
    /// purpose, because being quiet is the safe direction.
    #[test]
    fn does_not_flag_a_whole_named_inside_quoted_data() {
        assert!(
            report("(destructuring-bind (&whole all p) x (eval '(f all)))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_whole_referenced_from_a_lambda_list_init_form() {
        assert!(
            report("(destructuring-bind (&whole all p &optional (q (car all))) x (list p q))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_sharp_quoted_reference() {
        assert!(
            report("(destructuring-bind (&whole all p) x (mapcar #'all (list p)))")
                .findings
                .is_empty()
        );
    }

    /// A docstring naming the variable is not a reference — but it must not be
    /// read as one either, which is what would happen if string atoms
    /// normalized to bare symbols.
    #[test]
    fn a_mention_inside_a_string_is_not_a_reference() {
        let violations = report("(destructuring-bind (&whole all p) x \"all\" p)").findings;
        assert_eq!(violations.len(), 1);
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_lambda_list_without_a_whole() {
        assert!(
            report("(destructuring-bind (p q) x (list p q))")
                .findings
                .is_empty()
        );
    }

    /// CLHS requires `&whole` first; a `&whole` in a *nested* pattern has its
    /// own scope, which this reader does not model.
    #[test]
    fn does_not_flag_a_whole_in_a_nested_pattern() {
        assert!(
            report("(destructuring-bind (p (&whole inner a b)) x (list p a b))")
                .findings
                .is_empty()
        );
    }

    /// The discriminating case for "first, not merely present": a `&whole`
    /// written after a required parameter is a malformed lambda list, and a
    /// reader that searched for `&whole` anywhere would happily name `w` and
    /// report it. CLHS 3.4.4 makes the position part of the syntax, so a form
    /// that violates it is somebody else's complaint.
    #[test]
    fn does_not_flag_a_whole_written_after_a_required_parameter() {
        assert!(
            report("(destructuring-bind (p &whole w q) x (list p q))")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_conventionally_ignored_name() {
        assert!(
            report("(destructuring-bind (&whole _all p) x p)")
                .findings
                .is_empty()
        );
        assert!(
            report("(destructuring-bind (&whole _ p) x p)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_whole_whose_variable_is_not_a_symbol() {
        assert!(
            report("(destructuring-bind (&whole (a b) p) x p)")
                .findings
                .is_empty()
        );
    }

    /// CLHS makes the body forms optional (`form*`), so a `destructuring-bind`
    /// with none is well-formed — and its `&whole` is still unreferenced.
    #[test]
    fn flags_a_form_whose_body_is_empty() {
        assert_eq!(
            report("(destructuring-bind (&whole all p) x)")
                .findings
                .len(),
            1
        );
    }

    /// Two children is not a `destructuring-bind` at all: there is no
    /// expression to destructure.
    #[test]
    fn does_not_flag_a_form_missing_its_expression() {
        assert!(
            report("(destructuring-bind (&whole all p))")
                .findings
                .is_empty()
        );
    }

    /// The half this rule deliberately leaves to `inspect unused-parameters`.
    #[test]
    fn does_not_flag_a_macro_lambda_list() {
        assert!(
            report("(defmacro m (&whole w a b) (list a b))")
                .findings
                .is_empty()
        );
        assert!(
            report("(define-compiler-macro m (&whole w a) a)")
                .findings
                .is_empty()
        );
    }

    // -- the five quote shapes ------------------------------------------------

    #[test]
    fn plain_code_fires() {
        assert!(fires("(destructuring-bind (&whole all p) x p)"));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires("'(destructuring-bind (&whole all p) x p)"));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires("(quote (destructuring-bind (&whole all p) x p))"));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires("'(y ,(destructuring-bind (&whole all p) x p))"));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires("`(y ,(destructuring-bind (&whole all p) x p))"));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires(
            "(defmacro m () `(destructuring-bind (&whole all p) x p))"
        ));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = "(format t \"(destructuring-bind (&whole all p) x p)\")";
        assert!(report(source).findings.is_empty());
        assert!(!fires(source));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(destructuring-bind (&whole all p) x p)",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_destructuring_bind_unused_whole_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn the_summary_counts_every_whole_binding_not_only_the_flagged_ones() {
        let report = report(
            "(destructuring-bind (&whole a p) x p)\n\
             (destructuring-bind (&whole b p) x (list b p))\n\
             (destructuring-bind (p q) x (list p q))\n",
        );
        assert_eq!(report.summary, vec![("whole_binding_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_variable() {
        let report = report("(defun f (x)\n  (destructuring-bind (&whole all p) x p))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "destructuring-bind-unused-whole");
        assert_eq!(finding.text_columns(), vec!["all".to_owned()]);
    }
}
