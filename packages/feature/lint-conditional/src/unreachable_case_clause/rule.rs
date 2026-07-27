//! `unreachable-case-clause`: a case/typecase clause after a t/otherwise catch-all that can never run.
//!
//! The analysis lives in [`crate::unreachable_case_clause::domain`], which also backs the
//! standalone `inspect unreachable-case-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::unreachable_case_clause::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "unreachable-case-clause",
    RuleCategory::DeadCode,
    Severity::Error,
    "a case/typecase clause after a t/otherwise catch-all that can never run",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("case"), NormalizedHead::new("typecase")];

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
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} has {} unreachable clause(s) after a t/otherwise catch-all",
                    item.head, item.unreachable_count
                ),
            );
        }
        Ok(())
    }
}
