//! `duplicate-cond-tests`: a cond test expression repeated across more than one clause.
//!
//! The analysis lives in [`crate::domain::duplicate_cond_test_report`], which also backs the
//! standalone `inspect duplicate-cond-tests` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::duplicate_cond_test_report::examine_cond;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-cond-tests",
    RuleCategory::Duplicate,
    Severity::Error,
    "a cond test expression repeated across more than one clause",
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
                    "cond repeats test {} ({}×)",
                    item.test, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
