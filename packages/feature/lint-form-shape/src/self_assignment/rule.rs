//! `self-assignment`: a setq/setf/psetq/psetf that assigns a place to itself.
//!
//! The analysis lives in [`crate::self_assignment::domain`], which also backs the
//! standalone `inspect self-assignment` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::self_assignment::domain::examine_assignment;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "self-assignment",
    RuleCategory::Suspicious,
    Severity::Error,
    "a setq/setf/psetq/psetf that assigns a place to itself",
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
                format!("{} assigns place {} to itself", item.operator, item.place),
            );
        }
        Ok(())
    }
}
