//! `redundant-from-end-nil`: a sequence call with an explicit :from-end nil, the default ((find x seq :from-end nil) is (find x seq)).
//!
//! The analysis lives in [`crate::domain::redundant_from_end_nil_report`], which also backs the
//! standalone `inspect redundant-from-end-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_from_end_nil_report::examine;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-from-end-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a sequence call with an explicit :from-end nil, the default ((find x seq :from-end nil) is (find x seq))",
    Fixability::Fixable,
);

/// Sequence operators whose `:from-end` defaults to `nil`; mirrors
/// `FROM_END_HEADS` in the report module.
const HEADS: [NormalizedHead; 26] = [
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
    NormalizedHead::new("reduce"),
    NormalizedHead::new("search"),
    NormalizedHead::new("mismatch"),
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
                    "Drop the redundant :from-end nil".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} :from-end defaults to nil; drop the explicit :from-end nil",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
