//! `self-comparison`: a comparison whose two operands are structurally identical.
//!
//! The analysis lives in [`crate::domain::self_comparison_report`], which also backs the
//! standalone `inspect self-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::self_comparison_report::examine_comparison;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "self-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "a comparison whose two operands are structurally identical",
    Fixability::ReportOnly,
);

/// Mirrors `self_comparison_report::COMPARISON_HEADS`.
const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("eq"),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("equalp"),
    NormalizedHead::new("string="),
    NormalizedHead::new("char="),
    NormalizedHead::new("<"),
    NormalizedHead::new(">"),
    NormalizedHead::new("<="),
    NormalizedHead::new(">="),
    NormalizedHead::new("string<"),
    NormalizedHead::new("string>"),
    NormalizedHead::new("char<"),
    NormalizedHead::new("char>"),
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
                    "{} compares operand {} with itself",
                    item.operator, item.operand
                ),
            );
        }
        Ok(())
    }
}
