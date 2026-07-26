//! `duplicate-case-keys`: a case/ecase/ccase key repeated across more than one clause.
//!
//! The analysis lives in [`crate::domain::duplicate_case_key_report`], which also backs the
//! standalone `inspect duplicate-case-keys` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::duplicate_case_key_report::examine_case;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-case-keys",
    RuleCategory::Duplicate,
    Severity::Error,
    "a case/ecase/ccase key repeated across more than one clause",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("ccase"),
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
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} repeats key {} ({}×)",
                    item.head, item.key, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
