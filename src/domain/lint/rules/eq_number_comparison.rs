//! `eq-number-comparison`: an eq compared against a number literal (eq on numbers is unreliable).
//!
//! The analysis lives in [`crate::domain::eq_number_comparison_report`], which also backs the
//! standalone `inspect eq-number-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::eq_number_comparison_report::examine_comparison;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eq-number-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "an eq compared against a number literal (eq on numbers is unreliable)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eq")];

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
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, context.path(), &mut comparison_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::single(
                    item.head_span,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on numbers)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!("eq compares against number literal {}", item.literal),
                fix,
            );
        }
        Ok(())
    }
}
