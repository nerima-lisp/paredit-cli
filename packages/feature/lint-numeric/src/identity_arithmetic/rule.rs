//! `identity-arithmetic`: an arithmetic form with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1)).
//!
//! The analysis lives in [`crate::domain::identity_arithmetic_report`], which also backs the
//! standalone `inspect identity-arithmetic` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::identity_arithmetic_report::examine_form;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "identity-arithmetic",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an arithmetic form with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1))",
    Fixability::ReportOnly,
);

/// The four heads `examine_form` accepts.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("+"),
    NormalizedHead::new("*"),
    NormalizedHead::new("-"),
    NormalizedHead::new("/"),
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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, context.path(), &mut arithmetic_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} has a redundant identity operand {} (it does nothing)",
                    item.operator, item.identity
                ),
            );
        }
        Ok(())
    }
}
