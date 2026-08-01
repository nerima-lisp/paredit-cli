//! `loop-unreachable-finally-clause`: a `loop` epilogue form written after a
//! `finally` clause that already returns.
//!
//! The analysis lives in [`crate::loop_unreachable_finally_clause::domain`],
//! which also backs the standalone `inspect loop-unreachable-finally-clause`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::loop_syntax::LoopScan;
use crate::loop_unreachable_finally_clause::domain::examine_loop_unreachable_finally;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "loop-unreachable-finally-clause",
    RuleCategory::DeadCode,
    Severity::Error,
    "a loop epilogue form after a finally clause that already returns",
    // Deleting the dead form is not obviously the repair: the `return` being
    // written too early is at least as likely, and the two fixes produce
    // different values from the loop.
    Fixability::ReportOnly,
);

/// `examine_loop_unreachable_finally` only ever matches a `loop` head.
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
        examine_loop_unreachable_finally(view, &mut scan, &mut items);
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
