//! `identical-if-branches`: an if whose then and else branches are structurally identical.
//!

use paredit_core_lint_engine::LintResult;

use crate::identical_if_branches::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "identical-if-branches",
    RuleCategory::DeadCode,
    Severity::Error,
    "an if whose then and else branches are structurally identical",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

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
    ) -> LintResult<()> {
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(span, format!("if branches are identical: {}", item.branch));
        }
        Ok(())
    }
}
