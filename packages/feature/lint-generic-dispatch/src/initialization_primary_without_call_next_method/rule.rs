//! Registration for `initialization-primary-without-call-next-method`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::initialization_primary_without_call_next_method::domain::examine_defmethod;
use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "initialization-primary-without-call-next-method",
    RuleCategory::ObjectSystem,
    // An error, not a warning. Nothing signals — SBCL builds the instance and
    // hands it back with every slot unbound, including slots the caller passed
    // an `:initarg` for. The next reader of one of those slots gets
    // `UNBOUND-SLOT` from somewhere else entirely.
    Severity::Error,
    "a primary initialize-instance or shared-initialize method that never calls call-next-method",
    // No fix. Where in the body the call belongs decides what the method sees,
    // and whether the author meant `:after` instead is the actual question.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

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
        // The domain check is local and cheap; `is_unevaluated_at` reaches
        // `root_view()` and runs only once there is a finding.
        let Some(item) = examine_defmethod(view) else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view) {
            return Ok(());
        }
        sink.report(item.span, item.message());
        Ok(())
    }
}
