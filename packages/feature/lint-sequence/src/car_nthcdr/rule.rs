//! `car-nthcdr`: a car of an nthcdr, which is nth ((car (nthcdr n x)) is (nth n x)).
//!
//! The analysis lives in [`crate::car_nthcdr::domain`], which also backs the
//! standalone `inspect car-nthcdr` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::car_nthcdr::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "car-nthcdr",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a car of an nthcdr, which is nth ((car (nthcdr n x)) is (nth n x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("car")];

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
        let mut car_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut car_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (car (nthcdr n x)) is (nth n x).
                let text = format!(
                    "(nth {} {})",
                    context_slice(item.count_span),
                    context_slice(item.list_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (car (nthcdr n x)) as (nth n x)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "car of an nthcdr is nth; (car (nthcdr n x)) is (nth n x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
