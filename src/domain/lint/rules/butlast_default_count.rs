//! `butlast-default-count`: a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x)).
//!
//! The analysis lives in [`crate::domain::butlast_default_count_report`], which also backs the
//! standalone `inspect butlast-default-count` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::butlast_default_count_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "butlast-default-count",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("butlast"),
    NormalizedHead::new("nbutlast"),
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
                    "Drop the redundant butlast count of 1".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit count of 1 restates butlast's default; (butlast x 1) is (butlast x)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
