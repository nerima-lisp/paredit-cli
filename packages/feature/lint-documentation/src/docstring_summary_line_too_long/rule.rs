//! `docstring-summary-line-too-long`: a docstring whose first line is too wide
//! to serve as the summary every doc generator shows on its own.
//!
//! The analysis lives in [`crate::docstring_summary_line_too_long::domain`];
//! this module declares the rule's metadata, its head filter, and its knob.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::docstring_summary_line_too_long::domain::{MAX_WIDTH, examine};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "docstring-summary-line-too-long",
    // Descriptive metadata a definition carries and that does not do its job:
    // the docstring is present but its summary line is unusable as one.
    RuleCategory::Documentation,
    Severity::Warning,
    "a docstring whose first line is wider than the configured summary limit",
    // What a docstring should say — and where its line breaks belong — is not
    // something a rewrite can infer.
    Fixability::ReportOnly,
)
// How wide a summary may be is a house style, not a defect, and a project that
// has not decided will not want this on by default.
.with_tags(&[RuleTag::Pedantic, RuleTag::Style])
.with_settings(&[MAX_WIDTH])
.with_explanation(
    RuleExplanation::new(
        "Everything that lists documentation lists the docstring's *first line*: `apropos`, an \
         editor's echo-area hint, a generated API index. A first line long enough to be truncated \
         in all of them is the whole explanation with its line breaks missing, not a summary.",
    )
    .with_example(
        "(defun retry (n thunk) \"Attempt THUNK up to N times and return its first successful \
         value, re-signalling the last condition if every attempt fails.\" (funcall thunk))",
        "(defun retry (n thunk) \"Attempt THUNK up to N times.\n\nReturns its first successful \
         value, re-signalling the last condition if every attempt fails.\" (funcall thunk))",
    )
    .with_caveat(
        "A first line with no whitespace in it — a URL, one long symbol — is never reported: it \
         cannot be wrapped, so there is nothing to ask for.",
    ),
);

/// The six heads whose docstring position [`crate::support::docstring_place`]
/// reads. `defclass`/`define-condition`/`defgeneric` keep theirs in a
/// `(:documentation …)` option and `defstruct`'s collides with a slot name;
/// neither is read, so neither is listed.
const HEADS: [NormalizedHead; 6] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defmethod"),
    NormalizedHead::new("defvar"),
    NormalizedHead::new("defparameter"),
    NormalizedHead::new("defconstant"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let limit = context.setting(META.name().as_str(), MAX_WIDTH);
        // A negative or absurd override is clamped rather than refused: the
        // knob is validated against its declaration at argument-parsing time,
        // and a rule is not the place to re-litigate it.
        let limit = usize::try_from(limit).unwrap_or(0);

        let Some(found) = examine(view, limit) else {
            return Ok(());
        };
        // Last, and only once a finding is otherwise settled: a `(defun …)`
        // inside `'(…)` or inside a macro's `` `(…) `` template is data, and
        // its docstring is not one this file wrote.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(found.span, found.message());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule_with;
    use paredit_core_lint_engine::model::RuleSettings;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's dispatch — the thing that decides
    /// whether and how often `check` is called — is exercised for real. A
    /// domain test cannot catch a wrong `HeadFilter::Heads` list: a rule that
    /// declares the wrong head compiles, passes every domain test, and is
    /// simply never invoked by the CLI.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages_at(limit: i64, source: &str) -> Vec<String> {
        let mut settings = RuleSettings::new();
        settings.set("docstring-summary-line-too-long", "max", limit);
        run_rule_with(&ENTRIES, source, &settings)
    }

    fn wide(width: usize) -> String {
        let mut text = String::new();
        while text.chars().count() < width {
            text.push_str("ab ");
        }
        text.truncate(width);
        text
    }

    #[test]
    fn every_declared_head_reaches_a_finding_through_the_real_dispatch() {
        let long = wide(40);
        for source in [
            format!("(defun f (x) \"{long}\" (+ x 1))"),
            format!("(defmacro m (x) \"{long}\" x)"),
            format!("(defmethod area ((s square)) \"{long}\" 1)"),
            format!("(defvar *cache* nil \"{long}\")"),
            format!("(defparameter *timeout* 30 \"{long}\")"),
            format!("(defconstant +limit+ 10 \"{long}\")"),
        ] {
            assert_eq!(messages_at(20, &source).len(), 1, "no finding for {source}");
        }
    }

    /// The declared head list is package-qualifier- and case-insensitive
    /// because the engine's head index folds both before the lookup.
    #[test]
    fn a_qualified_or_upper_case_head_still_dispatches() {
        let long = wide(40);
        for head in ["cl:defun", "CL:DEFUN", "DEFUN"] {
            assert_eq!(
                messages_at(20, &format!("({head} f (x) \"{long}\" (+ x 1))")).len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    #[test]
    fn the_knob_moves_the_threshold() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", wide(40));
        assert_eq!(messages_at(20, &source).len(), 1);
        assert!(messages_at(100, &source).is_empty());
    }

    #[test]
    fn the_declared_default_applies_when_nothing_is_set() {
        let source = format!("(defun f (x) \"{}\" (+ x 1))", wide(200));
        let entries: &'static [RuleEntry] = &ENTRIES;
        assert_eq!(
            run_rule_with(entries, &source, &RuleSettings::new()).len(),
            1
        );
        let ordinary = "(defun add (x y) \"Return the sum of X and Y.\" (+ x y))";
        assert!(run_rule_with(entries, ordinary, &RuleSettings::new()).is_empty());
    }

    // --- the guard the domain tests cannot exercise

    /// The dispatcher hands a rule every head-matched node, quoted or not.
    /// Without the `is_unevaluated_at` call every one of these fires.
    #[test]
    fn no_finding_inside_any_of_the_five_quote_shapes() {
        let long = wide(40);
        for source in [
            format!("'(defun f (x) \"{long}\" (+ x 1))"),
            format!("(quote (defun f (x) \"{long}\" (+ x 1)))"),
            format!("'(a ,(defun f (x) \"{long}\" (+ x 1)))"),
            format!("`(defun f (x) \"{long}\" (+ x 1))"),
            format!("(defmacro def-thing (n) `(defun ,n (x) \"{long}\" (+ x 1)))"),
        ] {
            assert!(
                messages_at(20, &source).is_empty(),
                "fired on quoted data: {source}"
            );
        }
    }

    /// The one shape that is code again.
    #[test]
    fn an_unquoted_definition_inside_a_quasiquote_still_fires() {
        let source = format!("`(a ,(defun f (x) \"{}\" (+ x 1)))", wide(40));
        assert_eq!(messages_at(20, &source).len(), 1);
    }

    /// A comment is not a node, so a `;` line however long is invisible to a
    /// docstring rule.
    #[test]
    fn a_long_comment_is_not_a_docstring() {
        let source = format!(";; {}\n(defun f (x) (+ x 1))\n", wide(200));
        assert!(messages_at(20, &source).is_empty());
    }

    #[test]
    fn no_finding_on_a_form_that_is_not_a_definition() {
        let source = format!(
            "(let ((x \"{}\")) x)\n(format nil \"{}\")\n",
            wide(200),
            wide(200)
        );
        assert!(messages_at(20, &source).is_empty());
    }
}
