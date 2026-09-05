//! `dynamic-var-bound-across-thread-boundary`: a `let`-rebound special read on
//! a thread the binding does not reach.
//!

use paredit_core_lint_engine::LintResult;

use crate::dynamic_var_bound_across_thread_boundary::domain::examine_spawn;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "dynamic-var-bound-across-thread-boundary",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a thread body reading a special its enclosing let rebinds, which does not carry over",
    // Capturing the value lexically and rebinding inside the thunk needs a name
    // for the capture and a decision about where the rebinding goes.
    Fixability::ReportOnly,
);

/// `examine_spawn` only ever matches a `make-thread` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-thread")];

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
        let mut spawn_count = 0;
        let mut items = Vec::new();
        examine_spawn(context.tree(), view, &mut spawn_count, &mut items);
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
