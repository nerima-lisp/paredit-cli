//! `explicit-nil-return`: a return/return-from with an explicit nil result, the default ((return nil) is (return)).
//!
//! The analysis lives in [`crate::explicit_nil_return::domain`], which also backs the
//! standalone `inspect explicit-nil-return` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::explicit_nil_return::domain::examine_return;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "explicit-nil-return",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a return/return-from with an explicit nil result, the default ((return nil) is (return))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("return"),
    NormalizedHead::new("return-from"),
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
        let mut return_form_count = 0;
        let mut items = Vec::new();
        examine_return(view, context.source(), &mut return_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Drop the redundant nil result, preserving the operator (and block).
                let text = match item.block_span {
                    Some(block) => format!(
                        "({} {})",
                        context_slice(item.head_span),
                        context_slice(block)
                    ),
                    None => format!("({})", context_slice(item.head_span)),
                };

                RuleFix::single(item.span, text, "Drop the explicit nil result".to_owned())
            };
            let operator = item.operator;

            sink.report_fixed(
                span,
                format!("{operator} nil result is the default; drop the redundant nil"),
                fix,
            );
        }
        Ok(())
    }
}
