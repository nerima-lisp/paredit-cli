//! `self-comparison`: a comparison whose two operands are structurally identical.
//!

use paredit_core_lint_engine::LintResult;

use crate::self_comparison::domain::examine_comparison;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

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
                    "{} compares operand {} with itself",
                    item.operator, item.operand
                ),
            );
        }
        Ok(())
    }
}
