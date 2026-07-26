//! `dead-boolean-operand`: an and/or whose non-final constant operand makes later operands dead.
//!
//! The analysis lives in [`crate::domain::dead_boolean_operand_report`], which also backs the
//! standalone `inspect dead-boolean-operand` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::dead_boolean_operand_report::examine_boolean;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "dead-boolean-operand",
    RuleCategory::DeadCode,
    Severity::Error,
    "an and/or whose non-final constant operand makes later operands dead",
    Fixability::ReportOnly,
);

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
    ) -> Result<()> {
        let mut boolean_form_count = 0;
        let mut items = Vec::new();
        examine_boolean(view, context.path(), &mut boolean_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} short-circuits at literal {}; later operands are dead",
                    item.head, item.constant
                ),
            );
        }
        Ok(())
    }
}
