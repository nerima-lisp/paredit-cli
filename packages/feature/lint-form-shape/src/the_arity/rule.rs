//! `the-arity`: a the special form without exactly two arguments (a type and a form).
//!
//! The analysis lives in [`crate::the_arity::domain`], which also backs the
//! standalone `inspect the-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::the_arity::domain::examine_the;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "the-arity",
    RuleCategory::Arity,
    Severity::Error,
    "a the special form without exactly two arguments (a type and a form)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("the")];

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
        let mut the_form_count = 0;
        let mut items = Vec::new();
        examine_the(view, context.path(), &mut the_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "the takes exactly 2 arguments (a type and a form) but has {}",
                    item.argument_count
                ),
            );
        }
        Ok(())
    }
}
