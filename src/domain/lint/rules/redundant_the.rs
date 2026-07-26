//! `redundant-the`: a (the t form) type declaration, which is vacuous and is just form (t matches every object).
//!
//! The analysis lives in [`crate::domain::redundant_the_report`], which also backs the
//! standalone `inspect redundant-the` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_the_report::examine_the;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-the",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (the t form) type declaration, which is vacuous and is just form (t matches every object)",
    Fixability::Fixable,
);

/// `examine_the` only ever matches a `the` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("the")];

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
        let mut the_form_count = 0;
        let mut items = Vec::new();
        examine_the(view, context.path(), &mut the_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (the t form) is form: replace the whole declaration with the inner form.

                RuleFix::single(
                    item.span,
                    context_slice(item.form_span),
                    "Drop the vacuous (the t …) declaration".to_string(),
                )
            };

            sink.report_fixed(
                span,
                "(the t form) is a vacuous type declaration; it is just form".to_string(),
                fix,
            );
        }
        Ok(())
    }
}
