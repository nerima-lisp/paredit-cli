//! Common Lisp `multiple-value-setq` arity-disagreement detection: a variable
//! list whose length differs from a *literal* `(values …)` right-hand side.
//!
//! `(multiple-value-setq (a b c) (values 1 2))` is legal — CLHS says the
//! variables with no corresponding value are set to `nil`, and surplus values
//! are discarded — and that is exactly why it is worth reporting rather than
//! silently accepting: nothing signals, and a variable the author expected to
//! be assigned quietly becomes `nil`. Verified against SBCL, which runs the
//! form above and leaves `c` as `NIL` with no warning at all.
//!
//! Both directions are reported, because both are a disagreement between what
//! the author wrote on the left and what they wrote on the right, in one form,
//! with no indirection between them.
//!
//! # What this rule deliberately does not flag
//!
//! - **Anything but a literal `(values …)` right-hand side.** A call, a
//!   variable, `(values-list …)`, `(floor x y)` — the number of values is not
//!   readable from the source, and guessing would report on faith. This is the
//!   whole reason the rule is narrow enough to be sound.
//! - **A variable list that is not a `(…)` list of plain symbols.** A
//!   macro-produced list, or one containing a `(setf …)` place, is not
//!   something this reader claims to understand.
//! - **A `values` shadowed by a local `flet`/`labels`/`macrolet`.** Shadowing
//!   `cl:values` is a package-lock violation in every conforming
//!   implementation, so this is a documented limit rather than a live risk.
//! - **A form reached only as quoted data.**
//!
//! # Category
//!
//! `Suspicious`, not `Arity`. The `Arity` category is for "a call with an
//! argument count the operator cannot accept"; `multiple-value-setq` accepts
//! this one perfectly well and does something surprising with it, which is
//! precisely `Suspicious`'s "well-formed code whose meaning is probably not
//! what was intended". `setf-arity` and `the-arity` — the package's two `Arity`
//! rules — both report forms that are genuinely ill-formed, and neither reads
//! a `(values …)` operand at all.
//!
//! Scope: Common Lisp only.

use std::path::Path;

use paredit_core_lint_engine::LintResult;

use paredit_core_cli::report::{FileFindings, Finding};
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use paredit_core_syntax::view_query::{is_paren_list, list_head, symbol_in};
use serde_json::{Value, json};

use crate::support::{bindable_variable_name, child_call, for_each_evaluated_subview};

#[derive(Debug, Clone)]
pub struct MultipleValueSetqArityMismatchItem {
    /// The span of the whole `(multiple-value-setq …)` form.
    pub span: ByteSpan,
    /// How many variables the left-hand side names.
    pub variable_count: usize,
    /// How many values the literal `(values …)` produces.
    pub value_count: usize,
}

impl Finding for MultipleValueSetqArityMismatchItem {
    fn kind(&self) -> &'static str {
        "multiple-value-setq-arity-mismatch"
    }

    fn span(&self) -> ByteSpan {
        self.span
    }

    fn text_columns(&self) -> Vec<String> {
        vec![format!(
            "vars={} values={}",
            self.variable_count, self.value_count
        )]
    }

    fn json_fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("variable_count", json!(self.variable_count)),
            ("value_count", json!(self.value_count)),
        ]
    }

    fn message(&self) -> String {
        if self.variable_count > self.value_count {
            format!(
                "multiple-value-setq names {} variables but (values ...) produces {}; \
                 the surplus variable(s) are set to nil",
                self.variable_count, self.value_count
            )
        } else {
            format!(
                "multiple-value-setq names {} variables but (values ...) produces {}; \
                 the surplus value(s) are discarded",
                self.variable_count, self.value_count
            )
        }
    }
}

/// Every child of a `(…)` list read as a plain variable name, or `None` when
/// any of them is not one.
///
/// All-or-nothing on purpose: a single unreadable entry means the list's length
/// is not reliably the number of variables being assigned.
fn variable_names(list: &ExpressionView) -> Option<Vec<String>> {
    if !is_paren_list(list) || !list.reader_prefixes.is_empty() || list.children.is_empty() {
        return None;
    }
    list.children.iter().map(bindable_variable_name).collect()
}

/// Examines one node. Shared with the lint suite's rule.
///
/// Cheapest predicate first: head comparison, then arity of the form itself,
/// then the two operand shapes. Nothing allocates until both operands have been
/// confirmed readable.
pub fn examine(
    view: &ExpressionView,
    setq_form_count: &mut usize,
    violations: &mut Vec<MultipleValueSetqArityMismatchItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !symbol_in(head, &["multiple-value-setq"]) {
        return;
    }
    // `(multiple-value-setq (vars…) form)` — exactly two operands.
    if view.children.len() != 3 {
        return;
    }
    let Some(values) = child_call(view, 2, "values") else {
        return;
    };
    if !values.reader_prefixes.is_empty() {
        return;
    }
    let Some(variables) = variable_names(&view.children[1]) else {
        return;
    };
    // Only forms whose *both* sides are readable count towards the
    // denominator: they are the population this rule can say anything about.
    *setq_form_count += 1;

    let value_count = values.children.len() - 1;
    if variables.len() == value_count {
        return;
    }
    violations.push(MultipleValueSetqArityMismatchItem {
        span: view.span,
        variable_count: variables.len(),
        value_count,
    });
}

/// Collects every `multiple-value-setq` in one file whose variable list
/// disagrees with a literal `(values …)`, with the number of readable such
/// forms scanned as the denominator beside them.
pub fn build_multiple_value_setq_arity_mismatch_report(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<FileFindings<MultipleValueSetqArityMismatchItem>> {
    if dialect != Dialect::CommonLisp {
        return Ok(FileFindings::new(
            path.to_path_buf(),
            dialect,
            false,
            tree.source(),
            Vec::new(),
            vec![("setq_form_count", json!(0))],
        ));
    }

    let mut setq_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_evaluated_subview(&view, |subview| {
            examine(subview, &mut setq_form_count, &mut violations);
        });
    }

    Ok(FileFindings::new(
        path.to_path_buf(),
        dialect,
        true,
        tree.source(),
        violations,
        vec![("setq_form_count", json!(setq_form_count))],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::is_unevaluated_at;

    fn report(input: &str) -> FileFindings<MultipleValueSetqArityMismatchItem> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        build_multiple_value_setq_arity_mismatch_report(
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
    fn flags_more_variables_than_values() {
        let violations = report("(multiple-value-setq (a b c) (values 1 2))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].variable_count, 3);
        assert_eq!(violations[0].value_count, 2);
        assert!(violations[0].message().contains("set to nil"));
    }

    #[test]
    fn flags_more_values_than_variables() {
        let violations = report("(multiple-value-setq (a) (values 1 2))").findings;
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message().contains("discarded"));
    }

    #[test]
    fn flags_an_empty_values_call() {
        let violations = report("(multiple-value-setq (a) (values))").findings;
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].value_count, 0);
    }

    #[test]
    fn case_and_package_qualifier_fold() {
        assert_eq!(
            report("(CL:MULTIPLE-VALUE-SETQ (a b) (CL:VALUES 1))")
                .findings
                .len(),
            1
        );
    }

    // -- near-miss negatives -------------------------------------------------

    #[test]
    fn does_not_flag_a_matching_arity() {
        assert!(
            report("(multiple-value-setq (a b) (values 1 2))")
                .findings
                .is_empty()
        );
    }

    /// The guard that keeps this rule sound: a right-hand side whose value
    /// count is not readable from the source says nothing.
    #[test]
    fn does_not_flag_a_non_literal_right_hand_side() {
        for rhs in ["(floor x y)", "(values-list l)", "x", "(f)", "'(1 2)"] {
            let source = format!("(multiple-value-setq (a b c) {rhs})");
            assert!(report(&source).findings.is_empty(), "{rhs}");
        }
    }

    #[test]
    fn does_not_flag_an_unreadable_variable_list() {
        for vars in ["((setf (car x)))", "vars", "(a \"b\")", "(a :b)", "()"] {
            let source = format!("(multiple-value-setq {vars} (values 1))");
            assert!(report(&source).findings.is_empty(), "{vars}");
        }
    }

    #[test]
    fn does_not_flag_a_form_with_the_wrong_operand_count() {
        assert!(report("(multiple-value-setq (a b))").findings.is_empty());
        assert!(
            report("(multiple-value-setq (a b) (values 1) extra)")
                .findings
                .is_empty()
        );
    }

    #[test]
    fn does_not_flag_a_quoted_values_operand() {
        assert!(
            report("(multiple-value-setq (a b) '(values 1))")
                .findings
                .is_empty()
        );
    }

    /// `multiple-value-bind` is a different operator with a different rule.
    #[test]
    fn does_not_flag_multiple_value_bind() {
        assert!(
            report("(multiple-value-bind (a b c) (values 1 2) a)")
                .findings
                .is_empty()
        );
    }

    // -- the five quote shapes ------------------------------------------------

    #[test]
    fn plain_code_fires() {
        assert!(fires("(multiple-value-setq (a b) (values 1))"));
    }

    #[test]
    fn a_hard_quoted_form_is_silent() {
        assert!(!fires("'(multiple-value-setq (a b) (values 1))"));
    }

    #[test]
    fn a_long_hand_quote_form_is_silent() {
        assert!(!fires("(quote (multiple-value-setq (a b) (values 1)))"));
    }

    #[test]
    fn a_comma_inside_a_hard_quote_is_silent() {
        assert!(!fires("'(x ,(multiple-value-setq (a b) (values 1)))"));
    }

    #[test]
    fn an_unquote_inside_a_quasiquote_fires() {
        assert!(fires("`(x ,(multiple-value-setq (a b) (values 1)))"));
    }

    #[test]
    fn a_backquoted_template_is_silent() {
        assert!(!fires(
            "(defmacro m () `(multiple-value-setq (a b) (values 1)))"
        ));
    }

    // -- string literal -------------------------------------------------------

    #[test]
    fn a_form_spelled_only_inside_a_string_is_not_a_form() {
        let source = "(format t \"(multiple-value-setq (a b) (values 1))\")";
        assert!(report(source).findings.is_empty());
        assert!(!fires(source));
    }

    // -- report envelope ------------------------------------------------------

    #[test]
    fn a_non_common_lisp_dialect_is_reported_as_unmodelled() {
        let tree = SyntaxTree::parse_with_dialect(
            "(multiple-value-setq (a b) (values 1))",
            Dialect::Clojure,
        )
        .expect("parse");
        let report = build_multiple_value_setq_arity_mismatch_report(
            Path::new("app.clj"),
            Dialect::Clojure,
            &tree,
        )
        .expect("build report");
        assert!(!report.dialect_modelled);
        assert!(report.findings.is_empty());
    }

    /// The denominator counts the forms this rule *could* have reported — both
    /// operands readable — which is what makes it a usable coverage number.
    #[test]
    fn the_summary_counts_every_readable_form_not_only_the_flagged_ones() {
        let report = report(
            "(multiple-value-setq (a b) (values 1))\n\
             (multiple-value-setq (a b) (values 1 2))\n\
             (multiple-value-setq (a b) (floor x y))\n",
        );
        assert_eq!(report.summary, vec![("setq_form_count", json!(2))]);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn a_finding_carries_its_line_its_kind_and_its_counts() {
        let report = report("(defun f ()\n  (multiple-value-setq (a b) (values 1)))\n");
        let finding = &report.findings[0];
        assert_eq!(report.line_of(finding), 2);
        assert_eq!(finding.kind(), "multiple-value-setq-arity-mismatch");
        assert_eq!(
            finding.json_fields(),
            vec![("variable_count", json!(2)), ("value_count", json!(1))]
        );
    }
}
