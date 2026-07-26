//! `typecase-nil-key`: a typecase/etypecase/ctypecase clause with a bare nil type, which is the empty type and never matches (use null).
//!
//! The analysis lives in [`crate::domain::typecase_nil_key_report`], which also backs the
//! standalone `inspect typecase-nil-key` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::typecase_nil_key_report::examine_case;

pub const META: RuleMeta = RuleMeta::new(
    "typecase-nil-key",
    RuleCategory::DeadCode,
    Severity::Error,
    "a typecase/etypecase/ctypecase clause with a bare nil type, which is the empty type and never matches (use null)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("typecase"),
    NormalizedHead::new("etypecase"),
    NormalizedHead::new("ctypecase"),
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
        let mut typecase_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut typecase_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} clause type nil is the empty type and never matches; use null",
                    item.head
                ),
            );
        }
        Ok(())
    }
}
