//! `hash-table-iteration-order-assumed`: an element read by position out of a hash table's iteration, whose order is unspecified.
//!
//! The analysis lives in [`crate::hash_table_iteration_order_assumed::domain`],
//! which also backs the standalone `inspect
//! hash-table-iteration-order-assumed` command; this module only registers it
//! with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::hash_table_iteration_order_assumed::domain::{examine, message_for};
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "hash-table-iteration-order-assumed",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an element read by position out of a hash table's iteration, whose order is unspecified",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "CLHS 18.1 leaves the order of `maphash` and `loop … being the hash-keys` unspecified. A \
         list built from one and then read by position gives \"some key\", not \"the first key\": \
         the answer can differ between implementations, between versions of one implementation, \
         and between two tables with equal contents but different histories.",
    )
    .with_example(
        "(first (loop for k being the hash-keys of table collect k))",
        "(first (sort (loop for k being the hash-keys of table collect k) #'string<))",
    )
    .with_caveat(
        "A sorted result is the remedy, so it is never reported: neither (first (sort (hash-table-keys \
         table) #'string<)) nor a loop that sorts inside itself or hands its accumulation to a \
         `finally`. An order-blind use of the same list — its length, a membership test — is not \
         reported either. A `maphash` whose pushed list some later form reads positionally is the \
         same defect and is deliberately out of scope: correlating the two needs dataflow between \
         separate forms.",
    ),
);

/// The order-sensitive accessors. `car`, `first` and `nth` are dense in
/// ordinary code, which is why [`examine`] rejects a non-list operand before
/// reading anything.
const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("car"),
    NormalizedHead::new("first"),
    NormalizedHead::new("second"),
    NormalizedHead::new("third"),
    NormalizedHead::new("last"),
    NormalizedHead::new("nth"),
    NormalizedHead::new("elt"),
];

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
        let mut accessor_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut accessor_form_count, &mut items);
        for item in items {
            // Only now, with a finding already in hand: the dispatcher hands a
            // rule every head-matched node, quoted or not.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            sink.report(item.span, message_for(&item.accessor));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_lint_engine::policy::RuleDialectScope;
    use paredit_core_syntax::dialect::Dialect;

    #[test]
    fn is_report_only_and_common_lisp_scoped() {
        assert_eq!(META.fixability(), Fixability::ReportOnly);
        assert_eq!(META.severity(), Severity::Warning);
        assert_eq!(META.category(), RuleCategory::Suspicious);
        assert!(RULE.dialect_scope().includes(Dialect::CommonLisp));
        assert_eq!(RULE.dialect_scope(), RuleDialectScope::COMMON_LISP_ONLY);
    }

    /// The head filter must be `Heads`: the `clean/forms/*` benchmarks lint a
    /// file with no findings, and that measures exactly the per-file cost of a
    /// rule that matches nothing.
    #[test]
    fn the_head_filter_is_not_a_whole_tree_walk() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
    }
}
