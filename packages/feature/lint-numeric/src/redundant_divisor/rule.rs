//! `redundant-divisor`: a quotient op with a redundant divisor of 1 ((floor x 1) is (floor x)).
//!
//! The analysis lives in [`crate::redundant_divisor::domain`], which also backs the
//! standalone `inspect redundant-divisor` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_divisor::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-divisor",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a quotient op with a redundant divisor of 1 ((floor x 1) is (floor x))",
    Fixability::Fixable,
);

/// The quotient operators whose divisor defaults to `1` (mirrors
/// `redundant_divisor_report::QUOTIENT_OPS`).
const HEADS: [NormalizedHead; 8] = [
    NormalizedHead::new("floor"),
    NormalizedHead::new("ceiling"),
    NormalizedHead::new("truncate"),
    NormalizedHead::new("round"),
    NormalizedHead::new("ffloor"),
    NormalizedHead::new("fceiling"),
    NormalizedHead::new("ftruncate"),
    NormalizedHead::new("fround"),
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
    ) -> LintResult<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut quotient_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut quotient_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (floor x 1) is (floor x): drop the redundant unit divisor.
                let text = format!(
                    "({} {})",
                    context_slice(item.operator_span),
                    context_slice(item.number_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    format!("Drop the redundant 1 divisor from {}", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "the divisor defaults to 1; ({} x 1) is ({} x)",
                    item.operator, item.operator
                ),
                fix,
            );
        }
        Ok(())
    }
}
