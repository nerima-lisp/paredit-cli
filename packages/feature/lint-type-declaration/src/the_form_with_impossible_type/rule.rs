//! Registration for `the-form-with-impossible-type`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};
use crate::the_form_with_impossible_type::domain::examine_the;

pub const META: RuleMeta = RuleMeta::new(
    "the-form-with-impossible-type",
    RuleCategory::Declaration,
    // SBCL emits a full WARNING, not a style warning: `the` is an assertion the
    // optimiser may act on.
    Severity::Warning,
    "a (the TYPE EXPR) whose declared type cannot contain the value the expression plainly is",
    // Either the assertion or the expression is wrong, and the linter cannot
    // know which.
    Fixability::ReportOnly,
);

/// The one head this rule is about, and the reason its per-file cost is close to
/// nothing: a file with no `the` form never reaches `check` at all.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("the")];

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
        let Some(item) = examine_the(view) else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(item.span, item.message());
        Ok(())
    }
}
