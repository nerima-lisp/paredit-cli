//! `explicit-step-delta`: an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x)).
//!
//! The analysis lives in [`crate::explicit_step_delta::domain`], which also backs the
//! standalone `inspect explicit-step-delta` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::explicit_step_delta::domain::examine_step;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "explicit-step-delta",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x))",
    Fixability::Fixable,
);

/// The two heads `examine_step` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("incf"), NormalizedHead::new("decf")];

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
        let mut step_form_count = 0;
        let mut items = Vec::new();
        examine_step(view, context.path(), &mut step_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Drop the redundant delta: (incf place 1) -> (incf place).

                RuleFix::single(
                    item.span,
                    format!(
                        "({} {})",
                        context_slice(item.head_span),
                        context_slice(item.place_span)
                    ),
                    "Drop the explicit default delta of 1".to_owned(),
                )
            };
            let operator = item.operator;

            sink.report_fixed(
                span,
                format!("{operator} delta of 1 is the default; ({operator} x 1) is ({operator} x)"),
                fix,
            );
        }
        Ok(())
    }
}
