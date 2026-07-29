//! `if-to-unless`: a three-argument if with then=nil ((if c nil e) is (unless c e)).
//!
//! The analysis lives in [`crate::if_to_unless::domain`], which also backs the
//! standalone `inspect if-to-unless` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::if_to_unless::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "if-to-unless",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a three-argument if with then=nil ((if c nil e) is (unless c e))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

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
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (if c nil e) is (unless c e).
                let text = format!(
                    "(unless {} {})",
                    context_slice(item.test_span),
                    context_slice(item.else_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (if c nil e) as (unless c e)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "an if with a nil then-branch is an unless; (if c nil e) is (unless c e)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
