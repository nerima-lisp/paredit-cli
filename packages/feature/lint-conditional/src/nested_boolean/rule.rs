//! `nested-boolean`: a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c)).
//!
//! The analysis lives in [`crate::nested_boolean::domain`], which also backs the
//! standalone `inspect nested-boolean` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nested_boolean::domain::examine_boolean;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-boolean",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a same-operator and/or nested in an and/or, which flattens ((or a (or b c)) is (or a b c))",
    Fixability::Fixable,
);

/// `examine_boolean` only ever matches an `and` or `or` head.
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
        examine_boolean(view, context.source(), &mut boolean_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Splice the inner operands in place of the nested (op …) wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.inner_span).trim().to_owned(),
                    "Flatten the nested same-operator and/or".to_owned(),
                )
            };
            let operator = item.operator;

            sink.report_fixed(
                span,
                format!("{operator} nested in a {operator} flattens; its operands splice in"),
                fix,
            );
        }
        Ok(())
    }
}
