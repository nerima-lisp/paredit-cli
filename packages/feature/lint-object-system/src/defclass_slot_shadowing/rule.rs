//! `defclass-slot-shadowing`: a subclass slot that silently shadows a same-file
//! superclass slot.
//!

use paredit_core_lint_engine::LintResult;

use crate::defclass_slot_shadowing::domain::examine_defclass_slot_shadowing;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "defclass-slot-shadowing",
    RuleCategory::ObjectSystem,
    Severity::Warning,
    "a subclass slot that silently shadows a same-file superclass slot",
    // No fix. The two repairs — delete the redeclaration, or copy the parent's
    // `:initform` into it — mean different things, and choosing between them
    // needs the intent that made the slot appear twice.
    Fixability::ReportOnly,
);

/// `examine_defclass_slot_shadowing` only ever matches a `defclass` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defclass")];

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
        let mut defclass_form_count = 0;
        let mut items = Vec::new();
        examine_defclass_slot_shadowing(context.tree(), view, &mut defclass_form_count, &mut items);
        for item in items {
            sink.report(
                item.span,
                format!(
                    "slot {} in {} redeclares the slot {} declares, at the same allocation {}: \
                     per CLHS 7.5.3 this does not replace the inherited declaration — the \
                     :initarg and accessor sets are unioned, :type is conjoined, and :initform \
                     comes from the most specific declaration that supplies one",
                    item.slot, item.subclass, item.superclass, item.allocation
                ),
            );
        }
        Ok(())
    }
}
