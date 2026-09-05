//! `slot-value-bypasses-accessor`: a `slot-value` read of a slot the file
//! declares an accessor for.
//!

use paredit_core_lint_engine::LintResult;

use crate::slot_value_bypasses_accessor::domain::examine_slot_value_bypasses_accessor;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "slot-value-bypasses-accessor",
    RuleCategory::ObjectSystem,
    Severity::Warning,
    "a slot-value read of a slot the file declares an accessor for",
    // No fix, and the reason is specific rather than general. Rewriting
    // `(slot-value o 'x)` to `(x-of o)` is only correct when `x-of` is not
    // shadowed at that point by an enclosing `flet`/`labels`/`macrolet` — and a
    // rule under `HeadFilter::Heads` sees one node, not its enclosing binding
    // forms, so it cannot prove that the replacement preserves calls to local
    // functions with the accessor's name.
    Fixability::ReportOnly,
);

/// `examine_slot_value_bypasses_accessor` only ever matches a `slot-value`
/// head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("slot-value")];

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
        let mut slot_value_form_count = 0;
        let mut items = Vec::new();
        examine_slot_value_bypasses_accessor(
            context.tree(),
            view,
            &mut slot_value_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "slot-value reads {} directly although {} declares the accessor {}: this \
                     skips whatever that generic's method combination adds",
                    item.slot, item.class, item.accessor
                ),
            );
        }
        Ok(())
    }
}
