//! `defclass-required-slot-no-initform-or-initarg`: a slot with no `:initform`
//! and no `:initarg` that a method in the file reads.
//!

use paredit_core_lint_engine::LintResult;

use crate::defclass_required_slot_no_initform_or_initarg::domain::examine_defclass_required_slot_no_initform_or_initarg;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "defclass-required-slot-no-initform-or-initarg",
    RuleCategory::ObjectSystem,
    // A warning rather than an error, because the rule proves that the slot
    // *cannot be filled at construction* and that something reads it — not the
    // ordering that would make the read certainly unbound.
    Severity::Warning,
    "a slot with no :initform and no :initarg that a method in the file reads",
    // No fix. Adding an `:initform` invents a default value and adding an
    // `:initarg` invents a name, and neither is derivable from the source.
    Fixability::ReportOnly,
);

/// `examine_defclass_required_slot_no_initform_or_initarg` only ever matches a
/// `defclass` head.
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
        let mut slot_count = 0;
        let mut items = Vec::new();
        examine_defclass_required_slot_no_initform_or_initarg(
            context.tree(),
            view,
            &mut slot_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "slot {} of {} has neither :initform nor :initarg, so make-instance leaves \
                     it unbound, and the method {} reads it: the first read signals \
                     unbound-slot",
                    item.slot, item.class, item.read_by
                ),
            );
        }
        Ok(())
    }
}
