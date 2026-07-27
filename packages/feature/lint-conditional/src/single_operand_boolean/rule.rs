//! `single-operand-boolean`: a single-operand and/or ((and X) and (or X) are just X).
//!
//! The analysis lives in [`crate::single_operand_boolean::domain`], which also backs the
//! standalone `inspect single-operand-boolean` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_operand_boolean::domain::examine_boolean;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-boolean",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-operand and/or ((and X) and (or X) are just X)",
    Fixability::Fixable,
);

/// The two heads `examine_boolean` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("and"), NormalizedHead::new("or")];

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
        let mut boolean_form_count = 0;
        let mut items = Vec::new();
        examine_boolean(view, context.path(), &mut boolean_form_count, &mut items);
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
