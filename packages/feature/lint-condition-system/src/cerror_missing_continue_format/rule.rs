//! `cerror-missing-continue-format`: a continuable error nobody explained.
//!

use paredit_core_lint_engine::LintResult;

use crate::cerror_missing_continue_format::domain::examine_cerror;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cerror-missing-continue-format",
    RuleCategory::Conditions,
    Severity::Warning,
    "a cerror whose continue-format-control is missing or nil, defeating continuability",
    // The missing text is the whole point; only its author can write it.
    Fixability::ReportOnly,
);

/// `examine_cerror` only ever matches a `cerror` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cerror")];

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
        let mut cerror_call_count = 0;
        let mut items = Vec::new();
        examine_cerror(view, &mut cerror_call_count, &mut items);
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
