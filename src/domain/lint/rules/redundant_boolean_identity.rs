//! `redundant-boolean-identity`: an and/or with a redundant identity operand (t in and, nil in or; (and a t b) is (and a b)).
//!
//! The analysis lives in [`crate::domain::redundant_boolean_identity_report`], which also backs the
//! standalone `inspect redundant-boolean-identity` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_boolean_identity_report::examine_boolean;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-boolean-identity",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an and/or with a redundant identity operand (t in and, nil in or; (and a t b) is (and a b))",
    Fixability::Fixable,
);

/// The two boolean operators `examine_boolean` recognizes.
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
                // Reconstruct `(op kept…)` from the surviving operands, or collapse
                // to the bare identity when every operand was the identity.
                let kept: Vec<String> = item.kept_spans.iter().map(|s| context_slice(*s)).collect();
                let text = if kept.is_empty() {
                    item.identity.to_owned()
                } else {
                    format!("({} {})", item.operator, kept.join(" "))
                };

                RuleFix::single(
                    item.span,
                    text,
                    format!(
                        "Drop the redundant {} operand from {}",
                        item.identity, item.operator
                    ),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} has a redundant {} operand; drop it",
                    item.operator, item.identity
                ),
                fix,
            );
        }
        Ok(())
    }
}
