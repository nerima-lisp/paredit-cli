//! `setq-non-variable`: a setq/psetq place that is not a variable (a list, literal, or constant).
//!
//! The analysis lives in [`crate::domain::setq_non_variable_report`], which also backs the
//! standalone `inspect setq-non-variable` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::setq_non_variable_report::examine_setq;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "setq-non-variable",
    RuleCategory::Malformed,
    Severity::Error,
    "a setq/psetq place that is not a variable (a list, literal, or constant)",
    Fixability::ReportOnly,
);

/// The two heads `examine_setq` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("setq"), NormalizedHead::new("psetq")];

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
        let mut assignment_form_count = 0;
        let mut items = Vec::new();
        examine_setq(view, context.path(), &mut assignment_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("{} place {} is not a variable", item.operator, item.place),
            );
        }
        Ok(())
    }
}
