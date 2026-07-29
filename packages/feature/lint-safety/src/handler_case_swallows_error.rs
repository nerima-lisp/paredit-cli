//! `handler-case-swallows-error`: catching a condition and doing nothing.
//!
//! `(handler-case (risky) (error () nil))` turns every failure — including the
//! ones nobody anticipated — into a `nil` that the caller cannot distinguish
//! from a legitimate empty result. The error is gone: not logged, not
//! re-signalled, not counted.
//!
//! The condition type matters. Catching `file-not-found` and returning `nil` is
//! a decision about a known case; catching `error` and returning `nil` is a
//! decision about *every* case, including the ones the code has not been
//! written for yet. Only the broad types are reported, which is what keeps the
//! rule from firing on ordinary, careful error handling.
//!
//! "Doing nothing" is read narrowly: an empty body, or a body that is one
//! constant. A body that logs, returns a default, or converts the condition is
//! doing something, whatever this rule thinks of it.
//!
//! Report-only. What should happen instead is the entire question.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionKind, ExpressionView};
use paredit_core_syntax::view_query::{atom_text, list_head, symbol_in};

pub const META: RuleMeta = RuleMeta::new(
    "handler-case-swallows-error",
    RuleCategory::Conditions,
    Severity::Error,
    "a handler-case clause on a broad condition type whose body discards the condition entirely",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A handler on `error` catches every failure the body can signal, including ones written \
         after the handler. A body that returns a constant leaves the caller unable to tell \
         failure from a legitimate result, and leaves no record that anything went wrong.",
    )
    .with_example(
        "(handler-case (risky) (error () nil))",
        "(handler-case (risky) (error (c) (log-error c) nil))",
    )
    .with_caveat(
        "A handler on a *specific* condition type is never reported: catching `file-error` and \
         returning `nil` is a decision about a known case, which is what the condition system is \
         for.",
    ),
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("handler-case"),
    NormalizedHead::new("ignore-errors"),
];

/// Condition types broad enough that catching them is catching everything.
const BROAD_TYPES: [&str; 5] = ["error", "condition", "serious-condition", "t", "warning"];

/// One swallowed condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwallowedError {
    pub span: ByteSpan,
    pub condition_type: String,
}

/// Whether a clause body does nothing observable.
///
/// Empty, or a single atom that is a constant. A single *call* is doing
/// something — it may well be `(values)`, but this rule does not claim to know
/// what a function does.
fn body_is_inert(body: &[ExpressionView]) -> bool {
    match body {
        [] => true,
        [only] => {
            only.kind == ExpressionKind::Atom
                && atom_text(only).is_some_and(|text| {
                    text.eq_ignore_ascii_case("nil")
                        || text.eq_ignore_ascii_case("t")
                        || text.starts_with('"')
                        || text.starts_with(':')
                        || text.starts_with(|c: char| c.is_ascii_digit())
                })
        }
        _ => false,
    }
}

/// Every swallowed condition in one `handler-case`.
///
/// `ignore-errors` is the same defect with no clause to read — the whole form
/// *is* "catch `error`, return `nil`" — so it is reported whole.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<SwallowedError> {
    let Some(head) = list_head(view) else {
        return Vec::new();
    };

    if symbol_in(head, &["ignore-errors"]) {
        return vec![SwallowedError {
            span: view.span,
            condition_type: "error".to_owned(),
        }];
    }

    if !symbol_in(head, &["handler-case"]) {
        return Vec::new();
    }

    // `(handler-case protected-form clause…)`: the clauses start at index 2.
    view.children
        .iter()
        .skip(2)
        .filter_map(|clause| {
            let condition = clause.children.first().and_then(atom_text)?;
            if !symbol_in(condition, &BROAD_TYPES) {
                return None;
            }
            // `(type (var) body…)`: the variable list is always present, so the
            // body starts at index 2.
            let body = clause.children.get(2..).unwrap_or(&[]);
            body_is_inert(body).then(|| SwallowedError {
                span: clause.span,
                condition_type: paredit_core_syntax::view_query::unqualified(condition)
                    .to_ascii_lowercase(),
            })
        })
        .collect()
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for swallowed in examine(view) {
            sink.report(
                swallowed.span,
                format!(
                    "this catches {} and discards it, so every failure — including ones written \
                     later — becomes an indistinguishable result",
                    swallowed.condition_type
                ),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::{Path, SyntaxTree};

    fn types(input: &str) -> Vec<String> {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let view = tree
            .select_path(&Path::root_child(0))
            .expect("root form")
            .view();
        examine(&view)
            .into_iter()
            .map(|item| item.condition_type)
            .collect()
    }

    #[test]
    fn flags_a_broad_handler_returning_nil() {
        assert_eq!(
            types("(handler-case (risky) (error () nil))"),
            vec!["error"]
        );
        assert_eq!(
            types("(handler-case (risky) (condition (c) nil))"),
            vec!["condition"]
        );
    }

    #[test]
    fn flags_an_empty_handler_body() {
        assert_eq!(types("(handler-case (risky) (error ()))"), vec!["error"]);
    }

    #[test]
    fn flags_ignore_errors_whole() {
        assert_eq!(types("(ignore-errors (risky))"), vec!["error"]);
    }

    #[test]
    fn does_not_flag_a_handler_that_does_something() {
        assert!(types("(handler-case (risky) (error (c) (log c) nil))").is_empty());
        assert!(types("(handler-case (risky) (error (c) (report c)))").is_empty());
    }

    #[test]
    fn does_not_flag_a_specific_condition_type() {
        assert!(types("(handler-case (risky) (file-error () nil))").is_empty());
        assert!(types("(handler-case (risky) (my-app-error () nil))").is_empty());
    }

    #[test]
    fn a_constant_other_than_nil_is_still_inert() {
        assert_eq!(types("(handler-case (risky) (error () 0))"), vec!["error"]);
        assert_eq!(
            types(r#"(handler-case (risky) (error () "failed"))"#),
            vec!["error"]
        );
    }

    #[test]
    fn reads_every_clause() {
        assert_eq!(
            types("(handler-case (risky) (file-error (c) (retry c)) (error () nil))"),
            vec!["error"]
        );
    }

    #[test]
    fn a_handler_case_with_no_clauses_reports_nothing() {
        // That is `handler-case-no-clauses`' finding, not this one's.
        assert!(types("(handler-case (risky))").is_empty());
    }
}
