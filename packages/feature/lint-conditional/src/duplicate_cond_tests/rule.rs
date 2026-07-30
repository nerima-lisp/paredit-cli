//! `duplicate-cond-tests`: a cond test expression repeated across more than one clause.
//!
//! The analysis lives in [`crate::duplicate_cond_tests::domain`], which also backs the
//! standalone `inspect duplicate-cond-tests` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::duplicate_cond_tests::domain::examine_cond;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-cond-tests",
    RuleCategory::Duplicate,
    Severity::Error,
    "a cond test expression repeated across more than one clause",
    Fixability::ReportOnly,
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, &mut cond_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "cond repeats test {} ({}×)",
                    item.test, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
