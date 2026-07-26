//! `single-operand-list-op`: a single-argument append/nconc/list*, which returns its argument unchanged ((append x) is x).
//!
//! The analysis lives in [`crate::domain::single_operand_list_op_report`], which also backs the
//! standalone `inspect single-operand-list-op` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::single_operand_list_op_report::examine_form;

pub const META: RuleMeta = RuleMeta::new(
    "single-operand-list-op",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a single-argument append/nconc/list*, which returns its argument unchanged ((append x) is x)",
    Fixability::Fixable,
);

/// The three heads `examine_form` accepts.
const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("append"),
    NormalizedHead::new("nconc"),
    NormalizedHead::new("list*"),
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
        let context_slice = |span| context.slice(span).to_owned();
        let mut list_op_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, context.path(), &mut list_op_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (append x) is x: replace the whole form with the argument source.

                RuleFix::single(
                    item.span,
                    context_slice(item.arg_span),
                    format!("Drop the no-op single-argument {}", item.head),
                )
            };
            let head = item.head;

            sink.report_fixed(
                span,
                format!("{head} of one argument returns it unchanged; ({head} x) is x"),
                fix,
            );
        }
        Ok(())
    }
}
