//! `nested-get-chain`: nested gets reading one path, which is get-in ((get (get m :a) :b) is (get-in m [:a :b])).
//!
use paredit_core_lint_engine::LintResult;

use crate::nested_get_chain::domain::{examine, is_chain_link, message_for};
use crate::support::{enclosing_form, is_unevaluated_at};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-get-chain",
    RuleCategory::Suspicious,
    Severity::Warning,
    "nested gets reading one path, which is get-in ((get (get m :a) :b) is (get-in m [:a :b]))",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`get-in`'s two-argument arity is `(reduce get m ks)`, so a chain of two-operand `get`s \
         is exactly a `get-in` over the same path — same lookups, same order, same nil-punning. \
         Spelled as a vector the path reads left to right; spelled as nesting it has to be read \
         inside out.",
    )
    .with_example("(get (get (get m :a) :b) :c)", "(get-in m [:a :b :c])")
    .with_caveat(
        "Only chains in which every `get` has exactly two operands are reported: `get-in`'s \
         not-found applies to the whole path while an inner `get`'s applies to one step, so a \
         chain carrying one anywhere is left alone. A chain is reported once, at its outermost \
         `get`.",
    ),
);

/// One head. `get` is common in Clojure, which is why [`examine`] rejects a
/// non-`get` target before anything walks.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("get")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Clojure only: `get` and `get-in` are `clojure.core`, and neither the
    /// shape nor the rewrite means anything under the CLHS.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut get_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut get_form_count, &mut items);
        for item in items {
            // Both descents happen only now, with a candidate already in hand.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            // One finding per chain: a link whose own parent is a link is not
            // the outermost, and the outermost is the one that reports.
            let nested = enclosing_form(context.tree(), item.span).is_some_and(|parent| {
                is_chain_link(&parent) && parent.children[1].span == item.span
            });
            if nested {
                continue;
            }
            sink.report(item.span, message_for(item.depth));
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
