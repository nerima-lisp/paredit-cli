//! `append-nil`: a two-argument append with a nil tail, a fresh copy ((append x nil) is (copy-list x)).
//!
//! The analysis lives in [`crate::domain::append_nil_report`], which also backs the
//! standalone `inspect append-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::append_nil_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "append-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-argument append with a nil tail, a fresh copy ((append x nil) is (copy-list x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("append")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut append_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut append_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (append x nil) is (copy-list x).
                let text = format!("(copy-list {})", context_slice(item.list_span));

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (append x nil) as (copy-list x)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "append with a nil tail is a fresh copy; (append x nil) is (copy-list x)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
