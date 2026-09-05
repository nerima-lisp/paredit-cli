//! `nested-cond-flattenable`: a cond whose final t clause holds only another
//! cond.
//!

use paredit_core_lint_engine::LintResult;

use crate::nested_cond_flattenable::domain::examine_cond;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-cond-flattenable",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond whose final t clause holds only another cond, which splices into the outer clause list",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

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
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, &mut cond_form_count, &mut items);
        for item in items {
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            sink.report(
                item.span,
                format!(
                    "the final t clause holds only a cond; its {} clause(s) can be spliced into the outer cond",
                    item.inner_clauses
                ),
            );
        }
        Ok(())
    }
}
