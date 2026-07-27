//! `eval-when-situation`: an eval-when with an invalid situation (not :compile-toplevel/:load-toplevel/:execute).
//!
//! The analysis lives in [`crate::domain::eval_when_situation_report`], which also backs the
//! standalone `inspect eval-when-situation` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::eval_when_situation_report::examine_eval_when;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eval-when-situation",
    RuleCategory::Malformed,
    Severity::Error,
    "an eval-when with an invalid situation (not :compile-toplevel/:load-toplevel/:execute)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eval-when")];

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
        let mut eval_when_form_count = 0;
        let mut items = Vec::new();
        examine_eval_when(view, context.path(), &mut eval_when_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("eval-when situation {} is not valid", item.situation),
            );
        }
        Ok(())
    }
}
