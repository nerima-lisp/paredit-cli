//! `negated-if`: a three-argument if with a negated test ((if (not c) a b) is (if c b a)).
//!
//! The analysis lives in [`crate::negated_if::domain`], which also backs the
//! standalone `inspect negated-if` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::negated_if::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-if",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a three-argument if with a negated test ((if (not c) a b) is (if c b a))",
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
        examine_if(view, context.path(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite `(if (not X) A B)` as `(if X B A)`: drop the negation and
                // swap the branches, copying each subform's exact source.
                let text = format!(
                    "(if {} {} {})",
                    context_slice(item.test_span),
                    context_slice(item.else_span),
                    context_slice(item.then_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Drop the negated test and swap the if branches".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "if test is negated; (if (not c) a b) is (if c b a)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
