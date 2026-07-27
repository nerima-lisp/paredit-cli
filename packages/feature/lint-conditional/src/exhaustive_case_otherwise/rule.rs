//! `exhaustive-case-otherwise`: an ecase/ccase/etypecase/ctypecase with a forbidden t/otherwise clause.
//!
//! The analysis lives in [`crate::exhaustive_case_otherwise::domain`], which also backs the
//! standalone `inspect exhaustive-case-otherwise` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::exhaustive_case_otherwise::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "exhaustive-case-otherwise",
    RuleCategory::Malformed,
    Severity::Error,
    "an ecase/ccase/etypecase/ctypecase with a forbidden t/otherwise clause",
    Fixability::ReportOnly,
);

/// The four exhaustive case forms `examine_case` recognizes.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("ecase"),
    NormalizedHead::new("ccase"),
    NormalizedHead::new("etypecase"),
    NormalizedHead::new("ctypecase"),
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
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} does not permit a {} clause (it is exhaustive)",
                    item.head, item.designator
                ),
            );
        }
        Ok(())
    }
}
