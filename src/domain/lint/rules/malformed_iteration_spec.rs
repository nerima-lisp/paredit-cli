//! `malformed-iteration-spec`: a dolist/dotimes spec that is not a (var form [result]) list.
//!
//! The analysis lives in [`crate::domain::malformed_iteration_spec_report`], which also backs the
//! standalone `inspect malformed-iteration-spec` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::malformed_iteration_spec_report::examine_iteration;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "malformed-iteration-spec",
    RuleCategory::Malformed,
    Severity::Error,
    "a dolist/dotimes spec that is not a (var form [result]) list",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
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
        let mut iteration_form_count = 0;
        let mut items = Vec::new();
        examine_iteration(view, context.path(), &mut iteration_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} spec {} must be (var form [result])",
                    item.head, item.spec
                ),
            );
        }
        Ok(())
    }
}
