//! `negated-comparison`: a not/null of a two-argument numeric comparison ((not (= a b)) is (/= a b)).
//!
//! The analysis lives in [`crate::domain::negated_comparison_report`], which also backs the
//! standalone `inspect negated-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::negated_comparison_report::examine_negation;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a not/null of a two-argument numeric comparison ((not (= a b)) is (/= a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("not"), NormalizedHead::new("null")];

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
        let mut negation_form_count = 0;
        let mut items = Vec::new();
        examine_negation(view, context.path(), &mut negation_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Rewrite `(not (OP a b))` as `(COMPLEMENT a b)`, copying operands.

                RuleFix::single(
                    item.span,
                    format!(
                        "({} {})",
                        item.complement,
                        context_slice(item.operands_span)
                    ),
                    format!("Use the complement {} instead of negating", item.complement),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "negated comparison has a complement operator; use {}",
                    item.complement
                ),
                fix,
            );
        }
        Ok(())
    }
}
