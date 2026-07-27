//! `t-comparison`: an eq/eql/equal/equalp comparison against t (only matches the symbol T, not any true value).
//!
//! The analysis lives in [`crate::domain::t_comparison_report`], which also backs the
//! standalone `inspect t-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::t_comparison_report::examine_comparison;

pub const META: RuleMeta = RuleMeta::new(
    "t-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an eq/eql/equal/equalp comparison against t (only matches the symbol T, not any true value)",
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
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, context.path(), &mut comparison_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} against t matches only the symbol T, not any true value",
                    item.operator
                ),
            );
        }
        Ok(())
    }
}
