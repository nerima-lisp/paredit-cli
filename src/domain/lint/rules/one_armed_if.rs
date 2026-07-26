//! `one-armed-if`: a two-argument if with no else branch ((if test then) is (when test then)).
//!
//! The analysis lives in [`crate::domain::one_armed_if_report`], which also backs the
//! standalone `inspect one-armed-if` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::one_armed_if_report::examine_if;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "one-armed-if",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-argument if with no else branch ((if test then) is (when test then))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

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
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, context.path(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::single(
                    item.head_span,
                    "when".to_owned(),
                    "Rewrite the else-less if as when".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "if has no else branch; (if test then) is (when test then)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
