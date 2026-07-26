//! `identical-if-branches`: an if whose then and else branches are structurally identical.
//!
//! The analysis lives in [`crate::domain::identical_if_branch_report`], which also backs the
//! standalone `inspect identical-if-branches` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::identical_if_branch_report::examine_if;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "identical-if-branches",
    RuleCategory::DeadCode,
    Severity::Error,
    "an if whose then and else branches are structurally identical",
    Fixability::ReportOnly,
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
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, context.path(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(span, format!("if branches are identical: {}", item.branch));
        }
        Ok(())
    }
}
