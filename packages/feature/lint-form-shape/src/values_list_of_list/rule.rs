//! `values-list-of-list`: a values-list of a list constructor ((values-list (list a b)) is (values a b)).
//!
//! The analysis lives in [`crate::values_list_of_list::domain`], which also backs the
//! standalone `inspect values-list-of-list` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::values_list_of_list::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "values-list-of-list",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a values-list of a list constructor ((values-list (list a b)) is (values a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("values-list")];

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
        let mut values_list_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &mut values_list_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // (values-list (list a b)) is (values a b); an empty list -> (values).
                let text = match item.elements_span {
                    Some(span) => format!("(values {})", context_slice(span)),
                    None => "(values)".to_owned(),
                };

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (values-list (list …)) as (values …)".to_owned(),
                )
            };

            sink.report_fixed(span, "values-list of a fresh list is just values; (values-list (list a b)) is (values a b)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
