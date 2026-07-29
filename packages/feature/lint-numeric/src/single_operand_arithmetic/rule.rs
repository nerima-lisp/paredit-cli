//! `single-operand-arithmetic`: a single-operand +/* ((+ X) and (* X) are just X).
//!
//! The analysis lives in [`crate::single_operand_arithmetic::domain`], which also backs the
//! standalone `inspect single-operand-arithmetic` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_operand_arithmetic::domain::examine_arithmetic;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-arithmetic",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-operand +/* ((+ X) and (* X) are just X)",
    Fixability::Fixable,
);

/// The two heads `examine_arithmetic` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("+"), NormalizedHead::new("*")];

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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_arithmetic(
            view,
            context.source(),
            &mut arithmetic_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // Replace the wrapper with its sole operand, copied verbatim.

                RuleFix::single(
                    item.span,
                    context_slice(item.inner_span),
                    format!("Unwrap the single-operand {}", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} has a single operand; ({} X) is just X",
                    item.operator, item.operator
                ),
                fix,
            );
        }
        Ok(())
    }
}
