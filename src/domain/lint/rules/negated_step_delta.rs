//! `negated-step-delta`: an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x)).
//!
//! The analysis lives in [`crate::domain::negated_step_delta_report`], which also backs the
//! standalone `inspect negated-step-delta` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::negated_step_delta_report::examine_step;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-step-delta",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x))",
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
                let item = item.clone();
                // Flip the operator and drop the sign: (incf x -5) -> (decf x 5).
                let magnitude = context_slice(item.delta_span);
                let magnitude = magnitude.strip_prefix('-').unwrap_or(&magnitude);
                let text = format!(
                    "({} {} {})",
                    item.opposite,
                    context_slice(item.place_span),
                    magnitude
                );

                RuleFix::single(
                    item.span,
                    text,
                    format!("Use {} with a positive delta", item.opposite),
                )
            };
            let opposite = item.opposite;

            sink.report_fixed(
                span,
                format!("negative delta flips the operator; use {opposite} with a positive delta"),
                fix,
            );
        }
        Ok(())
    }
}
