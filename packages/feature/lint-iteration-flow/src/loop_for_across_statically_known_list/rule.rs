//! `loop-for-across-statically-known-list`: a `loop for … across` clause over
//! a value that is provably a list rather than a vector.
//!

use paredit_core_lint_engine::LintResult;

use crate::loop_for_across_statically_known_list::domain::examine_loop_for_across;
use crate::loop_syntax::LoopScan;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "loop-for-across-statically-known-list",
    RuleCategory::Malformed,
    Severity::Error,
    "a loop for-across clause over a value that is provably a list, not a vector",
    // Rewriting `across` to `in` looks mechanical and is not: whether the
    // author meant to walk a list or to build a vector is the actual question,
    // and only one of those two repairs keeps the surrounding code correct.
    Fixability::ReportOnly,
);

/// `examine_loop_for_across` only ever matches a `loop` head.
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
        examine_loop_for_across(view, &mut scan, &mut items);
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
