//! `car-reverse`: a car of a reverse, a wasteful full copy ((car (reverse x)) is (car (last x))).
//!
//! The analysis lives in [`crate::car_reverse::domain`], which also backs the
//! standalone `inspect car-reverse` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::car_reverse::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "car-reverse",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a car of a reverse, a wasteful full copy ((car (reverse x)) is (car (last x)))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("car"), NormalizedHead::new("first")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut accessor_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut accessor_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (car (reverse x)) is (car (last x)), keeping the outer accessor.
                let text = format!(
                    "({} (last {}))",
                    context_slice(item.accessor_span),
                    context_slice(item.list_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (car (reverse x)) as (car (last x))".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "car of a reverse copies the whole list to read one element; use (car (last x))"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
