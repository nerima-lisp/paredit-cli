//! `define-condition-missing-report-for-error-type`: an error with nothing to
//! say.
//!

use paredit_core_lint_engine::LintResult;

use crate::define_condition_missing_report_for_error_type::domain::examine_define_condition;
use crate::support::{LazyHierarchy, is_unevaluated_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "define-condition-missing-report-for-error-type",
    RuleCategory::Conditions,
    Severity::Warning,
    "an error subtype with no :report option and no same-file superclass supplying one",
    // The one thing a fix would have to synthesize is the sentence a human
    // reads when the program fails. There is no safe default for that.
    Fixability::ReportOnly,
);

/// `examine_define_condition` only ever matches a `define-condition` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("define-condition")];

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
        let hierarchy = LazyHierarchy::new(context.tree());
        let mut define_condition_form_count = 0;
        let mut items = Vec::new();
        examine_define_condition(
            view,
            &hierarchy,
            &mut define_condition_form_count,
            &mut items,
        );
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
