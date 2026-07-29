//! `append-list-to-cons`: a one-element append that is just a cons ((append (list x) rest) is (cons x rest)).
//!
//! The analysis lives in [`crate::append_list_to_cons::domain`], which also backs the
//! standalone `inspect append-list-to-cons` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::append_list_to_cons::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "append-list-to-cons",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a one-element append that is just a cons ((append (list x) rest) is (cons x rest))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("append")];

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
        let mut append_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut append_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (append (list x) rest) is (cons x rest).
                let text = format!(
                    "(cons {} {})",
                    context_slice(item.element_span),
                    context_slice(item.rest_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (append (list x) rest) as (cons x rest)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a one-element append is just a cons; (append (list x) rest) is (cons x rest)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
