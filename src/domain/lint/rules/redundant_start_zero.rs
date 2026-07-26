//! `redundant-start-zero`: a bounded-sequence call with an explicit :start 0, the default ((find x seq :start 0) is (find x seq)).
//!
//! The analysis lives in [`crate::domain::redundant_start_zero_report`], which also backs the
//! standalone `inspect redundant-start-zero` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_start_zero_report::examine;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-start-zero",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a bounded-sequence call with an explicit :start 0, the default ((find x seq :start 0) is (find x seq))",
    Fixability::Fixable,
);

/// Single-sequence operators whose `:start` defaults to `0`; mirrors
/// `START_HEADS` in the report module.
const HEADS: [NormalizedHead; 35] = [
    NormalizedHead::new("find"),
    NormalizedHead::new("find-if"),
    NormalizedHead::new("find-if-not"),
    NormalizedHead::new("position"),
    NormalizedHead::new("position-if"),
    NormalizedHead::new("position-if-not"),
    NormalizedHead::new("count"),
    NormalizedHead::new("count-if"),
    NormalizedHead::new("count-if-not"),
    NormalizedHead::new("remove"),
    NormalizedHead::new("remove-if"),
    NormalizedHead::new("remove-if-not"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-if"),
    NormalizedHead::new("delete-if-not"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("substitute-if"),
    NormalizedHead::new("substitute-if-not"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nsubstitute-if"),
    NormalizedHead::new("nsubstitute-if-not"),
    NormalizedHead::new("remove-duplicates"),
    NormalizedHead::new("delete-duplicates"),
    NormalizedHead::new("fill"),
    NormalizedHead::new("reduce"),
    NormalizedHead::new("parse-integer"),
    NormalizedHead::new("read-from-string"),
    NormalizedHead::new("string-upcase"),
    NormalizedHead::new("string-downcase"),
    NormalizedHead::new("string-capitalize"),
    NormalizedHead::new("nstring-upcase"),
    NormalizedHead::new("nstring-downcase"),
    NormalizedHead::new("nstring-capitalize"),
    NormalizedHead::new("write-string"),
    NormalizedHead::new("write-line"),
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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::multi(
                    "Drop the redundant :start 0".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} :start defaults to 0; drop the explicit :start 0",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
