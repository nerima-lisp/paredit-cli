//! Common Lisp literal-place detection: a modify macro (`incf`, `decf`, `push`,
//! `pop`, `pushnew`, `setf`, `psetf`) whose *place* argument is a
//! self-evaluating literal — `(incf 5)`, `(decf "x")`, `(push item 3)`,
//! `(pop :k)`, `(setf 5 x)`, `(psetf a 1 "s" 2)`. A place must be something
//! `setf` can store into (a variable, an accessor form, …); a number, string,
//! character, or keyword literal is not a place, so these forms fail at
//! macroexpansion. The mistake is usually a wrong argument (`(incf n 5)` typed
//! as `(incf 5)`) or confusing the value and place slots.
//!
//! Distinct from two neighbouring rules: `modify-macro-arity` checks the
//! *number* of arguments, and `setq-non-variable` checks `setq`/`psetq` places.
//! This rule checks the *place slot's value* for the increment/stack/assignment
//! macros.
//!
//! The place slots differ per operator — `incf`/`decf`/`pop` take one first,
//! `push`/`pushnew` one second (after the value), and `setf`/`psetf` take a
//! place at each odd argument (the first of every place/value pair). Only slots
//! that are actually present are inspected, so an arity-broken call is left to
//! `modify-macro-arity`. Only self-evaluating literals are flagged; a symbol
//! place (variable or constant) and an accessor-form place are left alone.
//!
//! Reuses the shared whole-tree walk from
//! [`crate::domain::view_query::for_each_subview`].
//!
//! Scope: Common Lisp only.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::sexpr::{ByteSpan, ExpressionView, Path as SexprPath, SyntaxTree};
use crate::domain::view_query::{atom_child, for_each_subview, list_head};

/// The 0-based indices of the place arguments for a modify macro, or `None` if
/// the head is not one of them. `setf`/`psetf` place a slot at each odd index
/// (the first element of every place/value pair).
fn place_indices(head: &str, child_count: usize) -> Option<Vec<usize>> {
    if head.eq_ignore_ascii_case("incf")
        || head.eq_ignore_ascii_case("decf")
        || head.eq_ignore_ascii_case("pop")
    {
        Some(vec![1])
    } else if head.eq_ignore_ascii_case("push") || head.eq_ignore_ascii_case("pushnew") {
        Some(vec![2])
    } else if head.eq_ignore_ascii_case("setf") || head.eq_ignore_ascii_case("psetf") {
        Some((1..child_count).step_by(2).collect())
    } else {
        None
    }
}

/// The canonical (lowercased) operator name for a modify-macro head.
fn operator_name(head: &str) -> &'static str {
    for name in ["incf", "decf", "pop", "push", "pushnew", "setf", "psetf"] {
        if head.eq_ignore_ascii_case(name) {
            return name;
        }
    }
    "modify"
}

fn is_keyword(text: &str) -> bool {
    text.starts_with(':') && text.len() > 1 && !text[1..].contains(':')
}

fn is_number_literal(text: &str) -> bool {
    text.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
    }) && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok())
}

/// Whether an atom's text is a self-evaluating literal (never a valid place).
fn is_literal_place(text: &str) -> bool {
    text.starts_with('"') || text.starts_with("#\\") || is_keyword(text) || is_number_literal(text)
}

#[derive(Debug, Clone)]
pub struct LiteralPlaceItem {
    pub path: PathBuf,
    /// The span of the whole `(incf …)`-style form.
    pub span: ByteSpan,
    /// The modify macro (`incf`, `decf`, `push`, `pop`, or `pushnew`).
    pub operator: &'static str,
    /// The offending literal place (its source text).
    pub place: String,
}

#[derive(Debug)]
pub struct LiteralPlaceSummary {
    pub modify_form_count: usize,
    pub violations: Vec<LiteralPlaceItem>,
}

#[derive(Debug, Clone, Copy)]
pub struct LiteralPlacePolicyOptions {
    fail_on_violation: bool,
}

impl LiteralPlacePolicyOptions {
    #[must_use]
    pub fn new(fail_on_violation: bool) -> Self {
        Self { fail_on_violation }
    }

    #[must_use]
    pub const fn fail_on_violation(self) -> bool {
        self.fail_on_violation
    }
}

#[derive(Debug)]
pub struct LiteralPlacePolicy {
    pub fail_on_violation: bool,
    pub modify_form_count: usize,
    pub violation_count: usize,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Examines one node. Shared with the lint suite's rule, which reaches every
/// node through the single dispatch pass instead of walking the tree again.
pub(crate) fn examine_modify(
    view: &ExpressionView,
    path: &Path,
    modify_form_count: &mut usize,
    violations: &mut Vec<LiteralPlaceItem>,
) {
    let Some(head) = list_head(view) else {
        return;
    };
    let Some(indices) = place_indices(head, view.children.len()) else {
        return;
    };
    *modify_form_count += 1;

    // Report the first literal place slot; a call with two literal places is
    // still one form's bug. A missing slot is an arity problem (a different
    // rule), and a list place is a form, so both are skipped.
    for index in indices {
        if let Some(place) = atom_child(view, index) {
            if is_literal_place(place) {
                violations.push(LiteralPlaceItem {
                    path: path.to_path_buf(),
                    span: view.span,
                    operator: operator_name(head),
                    place: place.to_owned(),
                });
                return;
            }
        }
    }
}

/// Collects every modify macro whose place is a self-evaluating literal, along
/// with the total number of modify-macro forms scanned.
pub fn collect_literal_places(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<(usize, Vec<LiteralPlaceItem>)> {
    if dialect != Dialect::CommonLisp {
        return Ok((0, Vec::new()));
    }

    let mut modify_form_count = 0;
    let mut violations = Vec::new();
    for index in 0..tree.root_children().len() {
        let view = tree.select_path(&SexprPath::root_child(index))?.view();
        for_each_subview(&view, |subview| {
            examine_modify(subview, path, &mut modify_form_count, &mut violations);
        });
    }
    Ok((modify_form_count, violations))
}

#[must_use]
pub fn summarize_literal_places(
    modify_form_count: usize,
    violations: Vec<LiteralPlaceItem>,
) -> LiteralPlaceSummary {
    LiteralPlaceSummary {
        modify_form_count,
        violations,
    }
}

#[must_use]
pub fn evaluate_literal_place_policy(
    options: LiteralPlacePolicyOptions,
    summary: &LiteralPlaceSummary,
) -> LiteralPlacePolicy {
    let violation_count = summary.violations.len();
    let mut violations = Vec::new();
    if options.fail_on_violation() && violation_count > 0 {
        violations.push(format!("violation_count {violation_count} exceeds 0"));
    }

    LiteralPlacePolicy {
        fail_on_violation: options.fail_on_violation(),
        modify_form_count: summary.modify_form_count,
        violation_count,
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places(input: &str) -> (usize, Vec<LiteralPlaceItem>) {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse input");
        collect_literal_places(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect literal places")
    }

    #[test]
    fn flags_incf_of_a_number() {
        let (count, violations) = places("(incf 5)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "incf");
        assert_eq!(violations[0].place, "5");
    }

    #[test]
    fn flags_decf_of_a_string_and_pop_of_a_keyword() {
        let (_, violations) = places("(decf \"x\")");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "decf");

        let (_, violations) = places("(pop :k)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "pop");
    }

    #[test]
    fn flags_push_value_into_a_literal_place() {
        // push takes (value place); the place is the second argument.
        let (_, violations) = places("(push item 3)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "push");
        assert_eq!(violations[0].place, "3");
    }

    #[test]
    fn flags_pushnew_literal_place() {
        let (_, violations) = places("(pushnew x 42)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "pushnew");
    }

    #[test]
    fn flags_a_char_literal_place() {
        let (_, violations) = places("(incf #\\a)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn does_not_flag_a_variable_place() {
        let (count, violations) = places("(incf n)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_an_accessor_form_place() {
        let (_, violations) = places("(incf (aref a i))");
        assert!(violations.is_empty());

        let (_, violations) = places("(push x (gethash k table))");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_push_value_that_is_a_literal() {
        // The literal here is the value being pushed, not the place.
        let (_, violations) = places("(push 3 stack)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_a_missing_place_slot() {
        // (push item) is arity-broken; that is modify-macro-arity's job.
        let (count, violations) = places("(push item)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn flags_setf_of_a_literal_place() {
        let (count, violations) = places("(setf 5 x)");
        assert_eq!(count, 1);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "setf");
        assert_eq!(violations[0].place, "5");
    }

    #[test]
    fn flags_a_literal_place_in_a_later_setf_pair() {
        // The first pair is fine; the second place is a literal.
        let (_, violations) = places("(setf ok 1 \"s\" 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].place, "\"s\"");
    }

    #[test]
    fn flags_psetf_of_a_literal_place() {
        let (_, violations) = places("(psetf a 1 :k 2)");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].operator, "psetf");
    }

    #[test]
    fn does_not_flag_setf_with_valid_places() {
        let (count, violations) = places("(setf x 1 (car y) 2 (gethash k h) 3)");
        assert_eq!(count, 1);
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_setf_value_that_is_a_literal() {
        // The literal here is the value, not the place.
        let (_, violations) = places("(setf x 5)");
        assert!(violations.is_empty());
    }

    #[test]
    fn does_not_flag_other_heads() {
        let (count, violations) = places("(setq 5 x)");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn case_folds_the_head() {
        let (_, violations) = places("(INCF 7)");
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse_with_dialect("(incf 5)", Dialect::Clojure).expect("parse");
        let (count, violations) =
            collect_literal_places(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
                .expect("collect literal places");
        assert_eq!(count, 0);
        assert!(violations.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let (count, items) = places("(incf 5)");
        let summary = summarize_literal_places(count, items);

        let quiet = evaluate_literal_place_policy(LiteralPlacePolicyOptions::new(false), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.violation_count, 1);

        let strict = evaluate_literal_place_policy(LiteralPlacePolicyOptions::new(true), &summary);
        assert!(!strict.passed);
    }
}
