//! `dotimes-bound-mutation-has-no-effect`: assigning the `dotimes` count
//! variable from inside the body.
//!

use paredit_core_lint_engine::LintResult;

use crate::dotimes_bound_mutation_has_no_effect::domain::examine_dotimes_bound_mutation;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "dotimes-bound-mutation-has-no-effect",
    RuleCategory::Suspicious,
    Severity::Warning,
    "assigning the dotimes count variable inside the body, which cannot change the iteration count",
    // The intended repair is usually a different loop form (`do`, or a `loop`
    // with a `while`), not an edit to this one. Deleting the assignment would
    // also drop whatever else the variable is used for.
    Fixability::ReportOnly,
);

/// `examine_dotimes_bound_mutation` only ever matches a `dotimes` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("dotimes")];

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
        let mut dotimes_form_count = 0;
        let mut items = Vec::new();
        examine_dotimes_bound_mutation(view, &mut dotimes_form_count, &mut items);
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
    /// `dotimes` form in it. `AllNodes` would cost one call per node and
    /// `WholeTree` one pass per file, both of them paid even when the rule
    /// matches nothing — which is precisely what the `clean/forms/*`
    /// benchmarks measure. Pinned here so the declaration cannot drift.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["dotimes"]);
    }
}
