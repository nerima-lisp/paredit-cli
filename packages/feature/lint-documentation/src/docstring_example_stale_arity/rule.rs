//! `docstring-example-stale-arity`: a worked example in a docstring calling the
//! function it documents with an argument count that function no longer takes.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::docstring_example_stale_arity::domain::examine;
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "docstring-example-stale-arity",
    // The defect is the *documentation* being wrong, not the code. `Arity`
    // describes a call the operator cannot accept; this call is inside a
    // string and is never evaluated, so nothing here is an arity error at
    // runtime — what is broken is the description.
    RuleCategory::Documentation,
    // Not `Pedantic`: unlike "should this have a docstring", there is no
    // project that wants its worked examples to be wrong.
    Severity::Warning,
    "a docstring example calling its own definition with an argument count the lambda list rejects",
    // What the example *should* say is a judgement about intent. Rewriting the
    // call to fit the current lambda list would invent argument values.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A worked example is the part of a docstring a reader trusts most, and the part nothing \
         checks. String contents are never rewritten by a rename or a signature change, so an \
         example goes stale silently and stays that way. Its arity, unlike the rest of a \
         docstring, is decidable: the lambda list is right there.",
    )
    .with_example(
        "(defun scale (factor) \"Example: (scale 3 2) => 6\" factor)",
        "(defun scale (factor) \"Example: (scale 2) => 6\" factor)",
    )
    .with_caveat(
        "Only calls to the definition's own name are read, and only from `defun` and `defmacro`. \
         An example carrying a placeholder (`...`, `&rest`, `…`) illustrates a shape rather than \
         a call and is never counted, and a `&key` lambda list is checked for under-supply only, \
         because `&allow-other-keys` gives it no numeric upper bound.",
    ),
);

/// Only the two forms whose lambda list is definitive for calls to their own
/// name. A `defmethod`'s is one of several congruent lambda lists on a generic
/// function, so an example written against the generic may legitimately not
/// match the method in hand.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
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
        let found = examine(view);
        if found.is_empty() {
            return Ok(());
        }
        // Last, and only once there is something to report: a `(defun …)`
        // inside a macro's `` `(…) `` template is a definition being *built*,
        // and its docstring is not one this file wrote.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in found {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule_with;
    use paredit_core_lint_engine::model::RuleSettings;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's own head index and dispatch sit
    /// between the test and the rule.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule_with(&ENTRIES, source, &RuleSettings::new())
    }

    #[test]
    fn both_declared_heads_reach_a_finding_through_the_real_dispatch() {
        assert_eq!(
            messages("(defun scale (x factor) \"Example: (scale 3)\" (* x factor))").len(),
            1
        );
        assert_eq!(
            messages("(defmacro twice (form) \"Example: (twice a b)\" form)").len(),
            1
        );
    }

    #[test]
    fn a_qualified_or_upper_case_head_still_dispatches() {
        for head in ["cl:defun", "CL:DEFUN", "DEFUN"] {
            assert_eq!(
                messages(&format!(
                    "({head} scale (x factor) \"Example: (scale 3)\" x)"
                ))
                .len(),
                1,
                "`{head}` did not reach a finding"
            );
        }
    }

    // --- the guard the domain tests cannot exercise

    /// A macro that *writes* a `defun` carries the definition as a template.
    /// Its docstring is spliced in at expansion time; judging it is judging
    /// something that is not there.
    #[test]
    fn no_finding_inside_any_of_the_five_quote_shapes() {
        for source in [
            "'(defun scale (x factor) \"Example: (scale 3)\" x)",
            "(quote (defun scale (x factor) \"Example: (scale 3)\" x))",
            "'(a ,(defun scale (x factor) \"Example: (scale 3)\" x))",
            "`(defun scale (x factor) \"Example: (scale 3)\" x)",
            "(defmacro defscale (n) `(defun ,n (x factor) \"Example: (scale 3)\" x))",
        ] {
            assert!(
                messages(source).is_empty(),
                "fired on quoted data: {source}"
            );
        }
    }

    #[test]
    fn an_unquoted_definition_inside_a_quasiquote_still_fires() {
        assert_eq!(
            messages("`(a ,(defun scale (x factor) \"Example: (scale 3)\" x))").len(),
            1
        );
    }

    /// A comment carrying the same stale example is invisible to this rule: a
    /// comment is not a node, and the docstring position is a node.
    #[test]
    fn a_stale_example_in_a_comment_is_not_this_rules_subject() {
        assert!(
            messages(";; Example: (scale 3)\n(defun scale (x factor) (* x factor))\n").is_empty()
        );
    }

    #[test]
    fn a_correct_example_produces_nothing_through_the_real_dispatch() {
        assert!(
            messages("(defun scale (x factor) \"Example: (scale 3 2) => 6\" (* x factor))")
                .is_empty()
        );
    }

    #[test]
    fn no_finding_on_a_form_that_is_not_a_definition() {
        assert!(messages("(let ((x \"(scale 1 2 3)\")) x)\n(defclass c () ())\n").is_empty());
    }
}
