//! `equality-arity`: an eq/eql/equal/equalp call without exactly two arguments.
//!
//! The analysis lives in [`crate::domain::equality_arity_report`], which also backs the
//! standalone `inspect equality-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::equality_arity_report::examine_call;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "equality-arity",
    RuleCategory::Arity,
    Severity::Error,
    "an eq/eql/equal/equalp call without exactly two arguments",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("eq"),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("equalp"),
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
        let mut call_count = 0;
        let mut items = Vec::new();
        examine_call(view, context.path(), &mut call_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} takes exactly 2 arguments but has {}",
                    item.operator, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
