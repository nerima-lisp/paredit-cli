//! `multiple-value-list-of-values`: a multiple-value-list of a values form ((multiple-value-list (values a b)) is (list a b)).
//!
//! The analysis lives in [`crate::multiple_value_list_of_values::domain`], which also backs the
//! standalone `inspect multiple-value-list-of-values` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::multiple_value_list_of_values::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "multiple-value-list-of-values",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multiple-value-list of a values form ((multiple-value-list (values a b)) is (list a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("multiple-value-list")];

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
        let mut mvl_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut mvl_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (multiple-value-list (values a b)) is (list a b); empty -> (list).
                let text = match item.elements_span {
                    Some(span) => format!("(list {})", context_slice(span)),
                    None => "(list)".to_owned(),
                };

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (multiple-value-list (values …)) as (list …)".to_owned(),
                )
            };

            sink.report_fixed(span, "multiple-value-list of a values form is just list; (multiple-value-list (values a b)) is (list a b)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
