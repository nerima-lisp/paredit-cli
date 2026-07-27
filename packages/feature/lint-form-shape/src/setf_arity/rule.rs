//! `setf-arity`: a setq/setf/psetq/psetf with an odd argument count.
//!
//! The analysis lives in [`crate::setf_arity::domain`], which also backs the
//! standalone `inspect setf-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::setf_arity::domain::examine_assignment;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "setf-arity",
    RuleCategory::Arity,
    Severity::Error,
    "a setq/setf/psetq/psetf with an odd argument count",
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
                    "{} has {} arguments; place/value pairs require an even count",
                    item.operator, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
