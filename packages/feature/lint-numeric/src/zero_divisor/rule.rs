//! `zero-divisor`: a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0)).
//!
//! The analysis lives in [`crate::zero_divisor::domain`], which also backs the
//! standalone `inspect zero-divisor` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::zero_divisor::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::value::evaluate_constant;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "zero-divisor",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a division-family form with a literal 0 divisor, a guaranteed division-by-zero ((/ x 0))",
    Fixability::ReportOnly,
);

/// Every operator `examine` accepts: `/` plus the quotient-family ops.
const HEADS: [NormalizedHead; 11] = [
    NormalizedHead::new("/"),
    NormalizedHead::new("mod"),
    NormalizedHead::new("rem"),
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
        // The value layer sees through a binding and through folded
        // arithmetic, so `(let ((z 0)) (/ x z))` and `(/ x (- 1 1))` are the
        // same bug as `(/ x 0)`. `Unknown` reads as "not provably zero", which
        // is what keeps this from firing on code that merely might divide by
        // zero at run time.
        let is_zero = |divisor: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                divisor,
                context.binding_table(),
                context.value_table(),
            )
            .is_integer(0)
        };

        let mut division_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &is_zero,
            &mut division_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} by a literal 0 always signals division-by-zero",
                    item.operator
                ),
            );
        }
        Ok(())
    }
}
