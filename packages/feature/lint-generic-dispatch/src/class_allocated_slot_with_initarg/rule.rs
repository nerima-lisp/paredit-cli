//! Registration for `class-allocated-slot-with-initarg`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::class_allocated_slot_with_initarg::domain::examine_defclass;
use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "class-allocated-slot-with-initarg",
    RuleCategory::ObjectSystem,
    // A warning rather than an error. The program is conforming and nothing
    // signals; what the rule reports is that two slot options contradict each
    // other about whether the value is per-instance or per-class, and it is
    // just possible to want the shared-write behaviour on purpose.
    Severity::Warning,
    "a :allocation :class slot that also accepts an :initarg, so one construction rewrites every \
     instance",
    // No fix. Dropping the `:initarg` and dropping the `:allocation :class` are
    // both one-line repairs, and which one is right is the finding.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defclass")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        // The slot walk is local and cheap; `is_unevaluated_at` reaches
        // `root_view()` and runs only once there is a finding.
        let items = examine_defclass(view);
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
