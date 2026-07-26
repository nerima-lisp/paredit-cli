//! `single-clause-cond`: a cond with one non-t clause that has a body ((cond (test a b)) is (when test a b)).
//!
//! The analysis lives in [`crate::domain::single_clause_cond_report`], which also backs the
//! standalone `inspect single-clause-cond` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::single_clause_cond_report::examine_cond;

pub const META: RuleMeta = RuleMeta::new(
    "single-clause-cond",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond with one non-t clause that has a body ((cond (test a b)) is (when test a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

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
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, context.path(), &mut cond_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Wrap the clause interior verbatim: (cond (test body…)) -> (when test body…).

                RuleFix::single(
                    item.span,
                    format!("(when {})", context_slice(item.clause_inner_span).trim()),
                    "Rewrite the single-clause cond as when".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "single-clause cond with a body is just when; (cond (test a b)) is (when test a b)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
