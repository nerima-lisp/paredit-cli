//! `loop-collect-into-immediately-returned`: a loop that collects into an accumulator only to return it from finally.
//!

use paredit_core_lint_engine::LintResult;

use crate::loop_collect_into_immediately_returned::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "loop-collect-into-immediately-returned",
    // Correct code that says in three clauses what one clause already does:
    // the same reading `redundant-let-star` and `empty-let` get.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a loop whose only collect ... into accumulator is returned unchanged by finally (return acc), which plain collect already does",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("loop")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first. [`examine`] places the clause keywords with
    /// one pass over the loop's *own* children — never its operands' subtrees —
    /// and the whole-form occurrence count runs only once the entire clause
    /// shape has matched. The quote descent runs last, once per finding.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut collect_into_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut collect_into_form_count, &mut items);
        if items.is_empty() || is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = paredit_core_cli::report::Finding::message(&item);
            sink.report(span, message);
        }
        Ok(())
    }
}
