//! `single-arg-comparison`: a numeric comparison (< > <= >= = /=) with one argument (always true; missing an operand?).
//!
//! The analysis lives in [`crate::domain::single_arg_comparison_report`], which also backs the
//! standalone `inspect single-arg-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::single_arg_comparison_report::examine_comparison;

pub const META: RuleMeta = RuleMeta::new(
    "single-arg-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "a numeric comparison (< > <= >= = /=) with one argument (always true; missing an operand?)",
    Fixability::ReportOnly,
);

/// Mirrors `single_arg_comparison_report::COMPARISONS`.
const HEADS: [NormalizedHead; 6] = [
    NormalizedHead::new("<"),
    NormalizedHead::new(">"),
    NormalizedHead::new("<="),
    NormalizedHead::new(">="),
    NormalizedHead::new("="),
    NormalizedHead::new("/="),
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
                    "{} has a single argument; the comparison is always true (missing an operand?)",
                    item.operator
                ),
            );
        }
        Ok(())
    }
}
