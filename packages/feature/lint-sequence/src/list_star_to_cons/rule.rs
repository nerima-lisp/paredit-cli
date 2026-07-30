//! `list-star-to-cons`: a two-argument list*, which is just a cons ((list* a b) is (cons a b)).
//!
//! The analysis lives in [`crate::list_star_to_cons::domain`], which also backs the
//! standalone `inspect list-star-to-cons` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::list_star_to_cons::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "list-star-to-cons",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-argument list*, which is just a cons ((list* a b) is (cons a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("list*")];

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
        let mut list_star_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut list_star_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (list* a b) is (cons a b).
                let text = format!(
                    "(cons {} {})",
                    context_slice(item.car_span),
                    context_slice(item.cdr_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (list* a b) as (cons a b)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a two-argument list* is just a cons; (list* a b) is (cons a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
