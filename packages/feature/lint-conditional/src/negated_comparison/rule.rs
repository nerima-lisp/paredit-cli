//! `negated-comparison`: a not/null of a two-argument numeric comparison ((not (= a b)) is (/= a b)).
//!
//! The analysis lives in [`crate::negated_comparison::domain`], which also backs the
//! standalone `inspect negated-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::negated_comparison::domain::examine_negation;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

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
