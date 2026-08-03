//! `racket-parameterize-empty-bindings`: a `parameterize` that rebinds nothing.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::parameterize_empty_bindings::domain::{DIALECTS, HEAD, MESSAGE, examine_parameterize};

pub const META: RuleMeta = RuleMeta::new(
    "racket-parameterize-empty-bindings",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a parameterize whose binding list is empty, so it rebinds nothing",
    Fixability::ReportOnly,
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
        let mut parameterize_form_count = 0;
        let mut items = Vec::new();
        examine_parameterize(
            context.tree(),
            view,
            &mut parameterize_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, MESSAGE.to_owned());
        }
        Ok(())
    }
}
