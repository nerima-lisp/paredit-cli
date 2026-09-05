//! `loop-clause-order-violation`: a `loop` variable clause after a main
//! clause, or a `named` clause that is not first.
//!

use paredit_core_lint_engine::LintResult;

use crate::loop_clause_order_violation::domain::examine_loop_clause_order;
use crate::loop_syntax::LoopScan;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "loop-clause-order-violation",
    RuleCategory::Malformed,
    Severity::Error,
    "a loop variable clause after a main clause, or a named clause that is not first",
    // Reordering `loop` clauses moves the forms that are evaluated per
    // iteration relative to the ones evaluated once, so a mechanical fix
    // cannot promise the loop still means what it meant. Report only.
    Fixability::ReportOnly,
);

/// `examine_loop_clause_order` only ever matches a `loop` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("loop")];

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
        let mut scan = LoopScan::default();
        let mut items = Vec::new();
        examine_loop_clause_order(view, &mut scan, &mut items);
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's head index is what keeps this rule off every file with no
    /// `loop` form in it. `AllNodes` would cost one call per node and
    /// `WholeTree` one pass per file, both of them paid even when the rule
    /// matches nothing — which is precisely what the `clean/forms/*`
    /// benchmarks measure. Pinned here so the declaration cannot drift.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["loop"]);
    }
}
