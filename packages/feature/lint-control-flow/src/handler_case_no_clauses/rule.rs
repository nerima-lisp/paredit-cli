//! `handler-case-no-clauses`: a handler-case with no handler clauses, which is just its body ((handler-case x) is x).
//!
//! The analysis lives in [`crate::handler_case_no_clauses::domain`], which also backs the
//! standalone `inspect handler-case-no-clauses` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::handler_case_no_clauses::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "handler-case-no-clauses",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a handler-case with no handler clauses, which is just its body ((handler-case x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("handler-case")];

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
        let mut handler_case_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &mut handler_case_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // (handler-case x) is x.

                RuleFix::single(
                    item.span,
                    context_slice(item.form_span),
                    "Drop the clauseless handler-case".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a handler-case with no clauses is just its body; (handler-case x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
