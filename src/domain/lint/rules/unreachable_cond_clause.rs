//! `unreachable-cond-clause`: a cond clause after a t catch-all that can never run.
//!
//! The analysis lives in [`crate::domain::unreachable_cond_clause_report`], which also backs the
//! standalone `inspect unreachable-cond-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::unreachable_cond_clause_report::examine_cond;

pub const META: RuleMeta = RuleMeta::new(
    "unreachable-cond-clause",
    RuleCategory::DeadCode,
    Severity::Error,
    "a cond clause after a t catch-all that can never run",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

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
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, context.path(), &mut cond_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "cond has {} unreachable clause(s) after a t clause",
                    item.unreachable_count
                ),
            );
        }
        Ok(())
    }
}
