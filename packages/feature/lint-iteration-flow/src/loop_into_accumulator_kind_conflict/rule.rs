//! `loop-into-accumulator-kind-conflict`: two `loop` accumulation clauses
//! building incompatible kinds into the same `into` variable.
//!
//! The analysis lives in
//! [`crate::loop_into_accumulator_kind_conflict::domain`], which also backs
//! the standalone `inspect loop-into-accumulator-kind-conflict` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::loop_into_accumulator_kind_conflict::domain::examine_loop_accumulator_conflict;
use crate::loop_syntax::LoopScan;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "loop-into-accumulator-kind-conflict",
    RuleCategory::Malformed,
    Severity::Error,
    "two loop accumulation clauses building incompatible kinds into the same into variable",
    // Which of the two clauses is the mistake is exactly the intent a machine
    // cannot infer: renaming either target changes what the loop returns.
    Fixability::ReportOnly,
);

/// `examine_loop_accumulator_conflict` only ever matches a `loop` head.
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
        examine_loop_accumulator_conflict(view, &mut scan, &mut items);
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
