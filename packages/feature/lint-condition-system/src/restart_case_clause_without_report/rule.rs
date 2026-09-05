//! `restart-case-clause-without-report`: a restart the debugger can only name.
//!

use paredit_core_lint_engine::LintResult;

use crate::restart_case_clause_without_report::domain::examine_restart_case;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "restart-case-clause-without-report",
    RuleCategory::Conditions,
    Severity::Warning,
    "a restart-case clause with no :report option",
    // What a restart *should* say is the one thing a rewrite cannot know.
    Fixability::ReportOnly,
);

/// `examine_restart_case` only ever matches a `restart-case` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("restart-case")];

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
        let mut restart_clause_count = 0;
        let mut items = Vec::new();
        examine_restart_case(view, &mut restart_clause_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only now: a `(restart-case …)` inside `'(…)` is a list of symbols.
        // Dispatch cannot tell, and asking costs a walk, so the question is
        // asked once a finding already exists rather than once per node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
