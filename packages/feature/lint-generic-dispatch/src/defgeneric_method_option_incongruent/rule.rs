//! Registration for `defgeneric-method-option-incongruent`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::defgeneric_method_option_incongruent::domain::examine_defgeneric;
use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "defgeneric-method-option-incongruent",
    RuleCategory::ObjectSystem,
    // An error, not a warning. SBCL 2.6.0 signals `SIMPLE-PROGRAM-ERROR` while
    // *evaluating the defgeneric itself* for every case this reports; the file
    // does not load.
    Severity::Error,
    "a (:method ...) option whose lambda list CLHS 7.6.4 will not let its own defgeneric accept",
    // No fix. Which of the two lambda lists is the wrong one is the question,
    // and adding a parameter to the method changes what the method can read
    // while adding one to the generic changes every other method's obligation.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defgeneric")];

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
        // The domain check is local to this form and cheap; `is_unevaluated_at`
        // reaches `root_view()`, which materializes the document, so it runs
        // only once there is a finding. A sibling package measured 450843
        // ns/call against 28 ns/call from that ordering alone.
        let items = examine_defgeneric(view, context.source());
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
