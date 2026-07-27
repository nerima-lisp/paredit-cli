//! `if-to-or`: an if whose test and then are the same atom ((if x x y) is (or x y)).
//!
//! The analysis lives in [`crate::if_to_or::domain`], which also backs the
//! standalone `inspect if-to-or` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::if_to_or::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "if-to-or",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an if whose test and then are the same atom ((if x x y) is (or x y))",
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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, context.path(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite `(if x x y)` as `(or x y)`, evaluating x once.
                let text = format!(
                    "(or {} {})",
                    context_slice(item.test_span),
                    context_slice(item.else_span)
                );

                RuleFix::single(item.span, text, "Rewrite (if x x y) as (or x y)".to_owned())
            };

            sink.report_fixed(
                span,
                "if returns its test or the else; (if x x y) is (or x y)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
