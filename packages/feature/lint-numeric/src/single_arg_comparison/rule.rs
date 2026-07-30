//! `single-arg-comparison`: a numeric comparison (< > <= >= = /=) with one argument (always true; missing an operand?).
//!
//! The analysis lives in [`crate::single_arg_comparison::domain`], which also backs the
//! standalone `inspect single-arg-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_arg_comparison::domain::examine_comparison;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, &mut comparison_form_count, &mut items);
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
