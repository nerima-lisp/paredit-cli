//! `duplicate-setf-places`: a setf/setq/psetf/psetq that assigns the same variable more than once ((setf a 1 a 2)).
//!
//! The analysis lives in [`crate::duplicate_setf_places::domain`], which also backs the
//! standalone `inspect duplicate-setf-places` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::duplicate_setf_places::domain::examine_assignment;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-setf-places",
    RuleCategory::Duplicate,
    Severity::Error,
    "a setf/setq/psetf/psetq that assigns the same variable more than once ((setf a 1 a 2))",
    Fixability::ReportOnly,
);

/// The four assignment heads `examine_assignment` accepts.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("setq"),
    NormalizedHead::new("psetq"),
    NormalizedHead::new("setf"),
    NormalizedHead::new("psetf"),
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
        let mut assignment_form_count = 0;
        let mut items = Vec::new();
        examine_assignment(view, context.path(), &mut assignment_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} assigns variable {} more than once; the earlier assignment is dead",
                    item.operator, item.place
                ),
            );
        }
        Ok(())
    }
}
