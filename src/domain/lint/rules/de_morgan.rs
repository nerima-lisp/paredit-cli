//! `de-morgan`: an and/or of all negations, collapsible by De Morgan ((and (not a) (not b)) is (not (or a b))).
//!
//! The analysis lives in [`crate::domain::de_morgan_report`], which also backs the
//! standalone `inspect de-morgan` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::de_morgan_report::examine_boolean;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "de-morgan",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an and/or of all negations, collapsible by De Morgan ((and (not a) (not b)) is (not (or a b)))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("and"), NormalizedHead::new("or")];

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
        let mut boolean_form_count = 0;
        let mut items = Vec::new();
        examine_boolean(view, context.path(), &mut boolean_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Rewrite `(and (not a) (not b))` as `(not (or a b))`, copying each
                // negation's inner operand.
                let inners: Vec<String> =
                    item.inner_spans.iter().map(|s| context_slice(*s)).collect();
                let text = format!("(not ({} {}))", item.opposite, inners.join(" "));

                RuleFix::single(
                    item.span,
                    text,
                    format!("Collapse the {} of negations via De Morgan", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} of negations collapses by De Morgan to (not ({} …))",
                    item.operator, item.opposite
                ),
                fix,
            );
        }
        Ok(())
    }
}
