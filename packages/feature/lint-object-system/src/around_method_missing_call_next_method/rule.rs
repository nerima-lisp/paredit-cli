//! `around-method-missing-call-next-method`: an `:around` method whose body
//! never calls `call-next-method`.
//!

use paredit_core_lint_engine::LintResult;

use crate::around_method_missing_call_next_method::domain::examine_around_method_missing_call_next_method;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "around-method-missing-call-next-method",
    RuleCategory::ObjectSystem,
    // A warning, not an error: an `:around` that deliberately short-circuits
    // the rest of the combination — a cache hit, a guard that refuses — is a
    // real idiom. What the rule reports is that the choice was made, not that
    // it was wrong.
    Severity::Warning,
    "an :around method whose body never calls call-next-method",
    // No fix. Inserting a `call-next-method` changes what the method returns
    // and when the rest of the combination runs, which is the whole question.
    Fixability::ReportOnly,
);

/// `examine_around_method_missing_call_next_method` only ever matches a
/// `defmethod` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

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
        let mut around_method_count = 0;
        let mut items = Vec::new();
        examine_around_method_missing_call_next_method(
            context.tree(),
            view,
            &mut around_method_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "the :around method on {} never calls call-next-method: it replaces every \
                     primary, :before and :after method rather than wrapping them",
                    item.generic
                ),
            );
        }
        Ok(())
    }
}
