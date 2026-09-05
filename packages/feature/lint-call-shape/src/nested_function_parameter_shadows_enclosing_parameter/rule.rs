//! `nested-function-parameter-shadows-enclosing-parameter`: an `flet`/`labels`
//! local function, or a nested `defun`, whose parameter reuses an enclosing
//! function's parameter name.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::nested_function_parameter_shadows_enclosing_parameter::domain::{
    examine_nested_definition, message,
};

pub const META: RuleMeta = RuleMeta::new(
    "nested-function-parameter-shadows-enclosing-parameter",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an flet/labels or nested-defun parameter reusing an enclosing function's parameter name",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style])
.with_explanation(
    RuleExplanation::new(
        "Inside the nested function the name no longer means what a reader of the enclosing one \
         expects, and nothing at the point of use says so. Renaming the inner parameter costs \
         nothing and removes the question.",
    )
    .with_example(
        "(defun draw (window) (flet ((paint (window) …)) (paint (main-window))))",
        "(defun draw (window) (flet ((paint (target) …)) (paint (main-window))))",
    )
    .with_caveat(
        "The recursive-helper idiom is not reported: when the enclosing form calls the nested \
         function with the shadowed variable in that very position — `(walk tree nil)` inside \
         `(defun f (tree) (labels ((walk (tree acc) …)) …))` — the reuse is deliberate. Nor is \
         anything reported when the nested function escapes as `#'name`, since its call sites \
         cannot then all be seen. An inline `(lambda (x) …)` is out of scope entirely.",
    ),
);

/// Exactly the heads `examine_nested_definition` reads. `defun` is here for the
/// nested-`defun` case; a *top-level* one is rejected by a span comparison that
/// allocates nothing.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("defun"),
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
    ) -> LintResult<()> {
        let mut nested_definition_count = 0;
        let mut items = Vec::new();
        examine_nested_definition(
            context.tree(),
            view,
            &mut nested_definition_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                message(&item.parameter, &item.inner_function, &item.outer_function),
            );
        }
        Ok(())
    }
}
