//! `step-zero`: an incf/decf with an explicit step of 0, a no-op ((incf x 0)).
//!
//! The analysis lives in [`crate::step_zero::domain`], which also backs the
//! standalone `inspect step-zero` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::step_zero::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "step-zero",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an incf/decf with an explicit step of 0, a no-op ((incf x 0))",
    Fixability::ReportOnly,
);

/// The two heads `examine` accepts.
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
        let mut step_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut step_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("{} by 0 is a no-op that changes nothing", item.operator),
            );
        }
        Ok(())
    }
}
