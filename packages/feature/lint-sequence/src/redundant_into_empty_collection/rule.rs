//! `redundant-into-empty-collection`: an into onto an empty vector or set, which is a direct conversion ((into [] coll) is (vec coll)).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_into_empty_collection::domain::{examine, message_for};
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-into-empty-collection",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an into onto an empty vector or set, which is a direct conversion ((into [] coll) is (vec coll))",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`(into [] coll)` conjes every element onto an empty vector, which is what `(vec coll)` \
         does and says. The direct conversion is one call rather than two and names the result \
         type instead of building it.",
    )
    .with_example("(into [] (map inc xs))", "(vec (map inc xs))")
    .with_caveat(
        "Only `[]` and `#{}` are reported. `conj` onto a list prepends, so `(into '() coll)` \
         reverses and has no order-preserving one-call equivalent; `(into {} pairs)` has no \
         single-function equivalent at all. The transducer arity `(into [] xform coll)` and any \
         non-empty target are left alone.",
    ),
);

/// One head, and not a dense one.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("into")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Clojure only: `into`, `vec` and `set` are `clojure.core`, and the shape
    /// means nothing under the CLHS.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut into_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut into_form_count, &mut items);
        for item in items {
            // Only now, with a finding already in hand: the dispatcher hands a
            // rule every head-matched node, quoted or not.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            sink.report(item.span, message_for(item.conversion));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn is_report_only_and_clojure_scoped() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
        assert_eq!(META.severity(), Severity::Warning);
        assert_eq!(META.category(), RuleCategory::Suspicious);
        assert_eq!(RULE.dialect_scope(), RuleDialectScope::CLOJURE_ONLY);
        assert!(RULE.dialect_scope().includes(Dialect::Clojure));
        assert!(!RULE.dialect_scope().includes(Dialect::CommonLisp));
    }

    #[test]
    fn the_head_filter_is_not_a_whole_tree_walk() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
    }
}
