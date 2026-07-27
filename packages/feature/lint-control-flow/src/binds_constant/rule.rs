//! `binds-constant`: a let/let*/do/do* binding whose variable is a constant (nil, t, or a keyword).
//!
//! The analysis lives in [`crate::binds_constant::domain`], which also backs the
//! standalone `inspect binds-constant` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::binds_constant::domain::examine_binding_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "binds-constant",
    RuleCategory::Malformed,
    Severity::Error,
    "a let/let*/do/do* binding whose variable is a constant (nil, t, or a keyword)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
];

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
        let mut binding_form_count = 0;
        let mut items = Vec::new();
        examine_binding_form(view, context.path(), &mut binding_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("{} cannot bind the constant {}", item.head, item.variable),
            );
        }
        Ok(())
    }
}
