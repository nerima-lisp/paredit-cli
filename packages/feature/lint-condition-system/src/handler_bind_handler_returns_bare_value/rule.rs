//! `handler-bind-handler-returns-bare-value`: a result nothing reads.
//!

use paredit_core_lint_engine::LintResult;

use crate::handler_bind_handler_returns_bare_value::domain::examine_handler_bind;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "handler-bind-handler-returns-bare-value",
    RuleCategory::Conditions,
    Severity::Warning,
    "a handler-bind handler ending in a bare value, which handler-bind discards",
    // The repair is a restart invocation or a non-local exit, and which one
    // depends entirely on what the surrounding program offers.
    Fixability::ReportOnly,
);

/// `examine_handler_bind` only ever matches a `handler-bind` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("handler-bind")];

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
        let mut handler_count = 0;
        let mut items = Vec::new();
        examine_handler_bind(view, &mut handler_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
