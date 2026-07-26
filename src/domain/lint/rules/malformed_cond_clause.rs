//! `malformed-cond-clause`: a cond clause that is not a non-empty list.
//!
//! The analysis lives in [`crate::domain::malformed_cond_clause_report`], which also backs the
//! standalone `inspect malformed-cond-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::malformed_cond_clause_report::examine_cond;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "malformed-cond-clause",
    RuleCategory::Malformed,
    Severity::Error,
    "a cond clause that is not a non-empty list",
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
                format!("cond clause {} is not a non-empty list", item.clause),
            );
        }
        Ok(())
    }
}
