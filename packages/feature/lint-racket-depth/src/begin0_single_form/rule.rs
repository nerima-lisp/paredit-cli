//! `racket-begin0-single-form`: a `begin0` that wraps exactly one expression.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::begin0_single_form::domain::{DIALECTS, HEAD, MESSAGE, examine_begin0};

pub const META: RuleMeta = RuleMeta::new(
    "racket-begin0-single-form",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a begin0 wrapping a single expression, which is just that expression",
    Fixability::Fixable,
);

const FILTER_HEADS: [NormalizedHead; 1] = [NormalizedHead::new(HEAD)];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILTER_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut begin0_form_count = 0;
        let mut items = Vec::new();
        examine_begin0(context.tree(), view, &mut begin0_form_count, &mut items);
        for item in items {
            // The inner form is copied from source rather than re-printed, so
            // its spacing, comments and reader prefixes survive the fix.
            let fix = RuleFix::single(
                item.span,
                context.slice(item.inner_span).to_owned(),
                "Unwrap the single-form begin0".to_owned(),
            );
            sink.report_fixed(item.span, MESSAGE.to_owned(), fix);
        }
        Ok(())
    }
}
