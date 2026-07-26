//! `binds-constant`: a let/let*/do/do* binding whose variable is a constant (nil, t, or a keyword).
//!
//! The analysis lives in [`crate::domain::binds_constant_report`], which also backs the
//! standalone `inspect binds-constant` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::binds_constant_report::examine_binding_form;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

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
