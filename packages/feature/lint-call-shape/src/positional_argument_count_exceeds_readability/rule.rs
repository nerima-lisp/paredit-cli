//! `positional-argument-count-exceeds-readability`: a call inside a definition
//! body passing a long run of unlabelled literal arguments.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    RuleTag, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::positional_argument_count_exceeds_readability::domain::{
    DEFAULT_MAX_POSITIONAL_LITERALS, examine_definition, message,
};

/// The knob: how many unlabelled literal arguments a call may carry.
pub const MAX_POSITIONAL_LITERALS: RuleSetting = RuleSetting::new(
    "max-positional-literals",
    DEFAULT_MAX_POSITIONAL_LITERALS as i64,
    "how many unlabelled literal arguments a call may pass before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "positional-argument-count-exceeds-readability",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a call passing a long run of unlabelled literal arguments of mixed kinds",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style, RuleTag::Pedantic])
.with_settings(&[MAX_POSITIONAL_LITERALS])
.with_explanation(
    RuleExplanation::new(
        "Nothing at a call site of unlabelled constants says which is which, so every reader has \
         to go and find the callee's lambda list. Keyword arguments put the names where they are \
         read, and cost nothing at the definition.",
    )
    .with_example(
        "(render-panel 10 20 \"status\" t 3)",
        "(render-panel :x 10 :y 20 :label \"status\" :visible t :depth 3)",
    )
    .with_caveat(
        "A homogeneous run of numbers is data — a matrix, a colour, a coordinate list — and is \
         never reported: `(matrix 1 0 0 0 1 0 0 0 1)` reads fine. So does any call with one \
         named argument in it, and any arbitrary-arity operator (`list`, `+`, `format`, `and`).",
    ),
);

/// The definition heads this rule anchors on. It reports *calls*, but a call
/// head cannot be enumerated, so the anchor is the definition whose body is
/// scanned — which keeps the rule on `Heads` and its cost proportional to the
/// matched subtree.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("defmethod"),
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
        let max_literals = context
            .setting(META.name().as_str(), MAX_POSITIONAL_LITERALS)
            .max(0) as usize;
        let mut definition_count = 0;
        let mut items = Vec::new();
        examine_definition(
            context.tree(),
            view,
            max_literals,
            &mut definition_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                message(&item.head, item.argument_count, item.threshold),
            );
        }
        Ok(())
    }
}
