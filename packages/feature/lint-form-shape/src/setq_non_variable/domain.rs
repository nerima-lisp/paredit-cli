//! Common Lisp setq-non-variable detection: a `setq` or `psetq` whose place is
//! not a plain variable symbol. Unlike `setf`, which assigns to a *place form*
//! (`(setf (car x) 5)`), `setq`/`psetq` require each place to be a symbol
//! naming a variable. A list place (`(setq (car x) 5)` — `setf` was meant), a
//! literal (`(setq 5 x)`), a constant (`(setq nil 1)`, `(setq :k 1)`), or a
//! quoted symbol (`(setq 'x 5)`) is a program error, caught at macroexpansion
//! rather than by the reader.
//!
//! Only the place positions (the even-indexed arguments) are inspected. Forms
//! whose place/value pairing is not statically visible are skipped to avoid
//! false positives: a quoted/quasiquoted `setq`, and any form with a `#+`/`#-`
//! reader conditional or `,@` splice argument, which shifts the pairing.
//!
//! Complements [`crate::setf_arity::domain`] (which checks the argument
//! *count*) by checking each place's *validity*.
//!
//! Reuses the shared whole-tree walk from
//! [`paredit_core_syntax::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use paredit_core_lint_engine::LintResult;

use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::expression_equality::render_expression;
use paredit_core_syntax::sexpr::{
    ByteSpan, ExpressionView, Path as SexprPath, ReaderPrefix, SyntaxTree,
};
use paredit_core_syntax::view_query::{atom_text, for_each_subview, list_head};

const SETQ_HEADS: [&str; 2] = ["setq", "psetq"];

/// Whether an argument's reader prefix or `#+`/`#-` marker makes the static
/// place/value pairing unreliable.
fn is_pairing_ambiguous(view: &ExpressionView) -> bool {
    let ambiguous_prefix = view.reader_prefixes.iter().any(|prefix| {
        matches!(
            prefix,
            ReaderPrefix::ReaderConditional
                | ReaderPrefix::ReaderConditionalSplicing
                | ReaderPrefix::UnquoteSplicing
        )
    });
    ambiguous_prefix
        || atom_text(view).is_some_and(|text| text.starts_with("#+") || text.starts_with("#-"))
}

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

/// Whether a place form is not a valid `setq` variable: a quoted form, a list,
/// a literal (number/string/character), or a constant (`nil`, `t`, a keyword).
fn place_is_invalid(view: &ExpressionView) -> bool {
    // A quoted/quasiquoted/unquoted place (`'x`) is not a variable.
    if !view.reader_prefixes.is_empty() {
        return true;
    }
    let Some(text) = atom_text(view) else {
        // A list place — `setf` territory, not `setq`.
        return true;
    };
    if is_number_literal(text) || text.starts_with('"') || text.starts_with("#\\") {
        return true;
    }
    // A constant variable cannot be assigned: nil, t, or a keyword.
    text.eq_ignore_ascii_case("t")
        || text.eq_ignore_ascii_case("nil")
        || (text.len() > 1 && text.starts_with(':') && text.as_bytes()[1] != b':')
}

#[derive(Debug, Clone)]
pub struct SetqNonVariableItem {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub operator: String,
    pub place: String,
}

#[derive(Debug)]
pub struct SetqNonVariableSummary {
    pub assignment_form_count: usize,
    pub violations: Vec<SetqNonVariableItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct SetqNonVariablePolicyOptions {
    fail_on_violation: bool,
}

impl SetqNonVariablePolicyOptions {
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
pub struct SetqNonVariablePolicy {
    pub fail_on_violation: bool,
    pub assignment_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub fn examine_setq(
    view: &ExpressionView,
    path: &Path,
    assignment_form_count: &mut usize,
    violations: &mut Vec<SetqNonVariableItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    if !SETQ_HEADS
        .iter()
        .any(|name| head.eq_ignore_ascii_case(name))
    {
        return;
    }
    // A quoted/quasiquoted setq is data or a template, not a call.
    if !view.reader_prefixes.is_empty() {
        return;
    }
    // A `#+`/`#-` or `,@` argument shifts the place/value pairing.
    if view.children.iter().skip(1).any(is_pairing_ambiguous) {
        return;
    }
    *assignment_form_count += 1;

    // Places are the even-indexed arguments: children 1, 3, 5, ...
    for place in view.children.iter().skip(1).step_by(2) {
        if place_is_invalid(place) {
            violations.push(SetqNonVariableItem {
                path: path.to_path_buf(),
                span: place.span,
                operator: head.to_owned(),
                place: render_expression(place),
            });
        }
    }
}

/// Collects every `setq`/`psetq` place that is not a variable across a whole
/// file, along with the total number of `setq`/`psetq` forms scanned.
pub fn collect_setq_non_variables(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> LintResult<(usize, Vec<SetqNonVariableItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut assignment_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_setq(subview, path, &mut assignment_form_count, &mut violations);
        });
    }
    Ok((assignment_form_count, violations))
}

#[must_use]
pub const fn summarize_setq_non_variables(
    assignment_form_count: usize,
    violations: Vec<SetqNonVariableItem>,
) -> SetqNonVariableSummary {
    SetqNonVariableSummary {
        assignment_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_setq_non_variable_policy(
    options: SetqNonVariablePolicyOptions,
    summary: &SetqNonVariableSummary,
) -> SetqNonVariablePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    SetqNonVariablePolicy {
        fail_on_violation: options.fail_on_violation(),
        assignment_form_count: summary.assignment_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn violations(input: &str) -> (usize, Vec<SetqNonVariableItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_setq_non_variables(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect setq non variables")
    }

    #[test]
    fn flags_a_list_place() {
        let (form_count, items) = violations("(setq (car x) 5)");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "setq");
        assert_eq!(items[0].place, "(car x)");
    }

    #[test]
    fn flags_a_literal_place() {
        let (_, items) = violations("(setq 5 x)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].place, "5");
    }

    #[test]
    fn flags_a_constant_place() {
        let (_, nil_place) = violations("(setq nil 1)");
        assert_eq!(nil_place.len(), 1);
        let (_, keyword_place) = violations("(setq :k 1)");
        assert_eq!(keyword_place.len(), 1);
    }

    #[test]
    fn flags_a_quoted_place() {
        let (_, items) = violations("(setq 'x 5)");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn flags_a_bad_place_among_good_ones() {
        let (_, items) = violations("(setq x 1 (car y) 2 z 3)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].place, "(car y)");
    }

    #[test]
    fn does_not_flag_ordinary_variables() {
        let (form_count, items) = violations("(setq x 1 y 2)");
        assert_eq!(form_count, 1);
        assert!(items.is_empty());
    }

    #[test]
    fn does_not_flag_a_special_variable() {
        let (_, items) = violations("(setq *global* 1)");
        assert!(items.is_empty());
    }

    #[test]
    fn flags_a_psetq_place() {
        let (_, items) = violations("(psetq (aref v i) 1)");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].operator, "psetq");
    }

    #[test]
    fn skips_a_reader_conditional_form() {
        let (form_count, items) = violations("(setq #+sbcl x #-sbcl y 5)");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_a_quoted_setq_form() {
        let (form_count, items) = violations("(list '(setq 5 x))");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn finds_a_setq_nested_in_a_function_body() {
        let (form_count, items) = violations("(defun f () (setq (car y) 1))");
        assert_eq!(form_count, 1);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree =
            SyntaxTree::parse_with_dialect("(setq 5 x)", Dialect::Clojure).expect("parse input");
        let (form_count, items) =
            collect_setq_non_variables(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect setq non variables");
        assert_eq!(form_count, 0);
        assert!(items.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (form_count, items) = violations("(setq 5 x)");
        let summary = summarize_setq_non_variables(form_count, items);

        let quiet =
            evaluate_setq_non_variable_policy(SetqNonVariablePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict =
            evaluate_setq_non_variable_policy(SetqNonVariablePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
