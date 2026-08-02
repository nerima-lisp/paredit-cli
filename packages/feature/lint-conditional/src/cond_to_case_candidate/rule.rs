//! `cond-to-case-candidate`: a cond whose every test compares one variable
//! against a literal.
//!
//! The analysis lives in [`crate::cond_to_case_candidate::domain`], which also
//! backs the standalone `inspect cond-to-case-candidate` command; this module
//! only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::cond_to_case_candidate::domain::examine_cond;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cond-to-case-candidate",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond whose every test compares one variable against a literal (case says it directly)",
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, &mut cond_form_count, &mut items);
        for item in items {
            // Only now, with a finding in hand, is the depth-many walk from the
            // root worth paying for. The dispatcher hands a rule every
            // head-matched node, including the ones inside `'(…)`.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            sink.report(
                item.span,
                format!(
                    "every cond test compares {} against a literal; case says this directly",
                    item.variable
                ),
            );
        }
        Ok(())
    }
}
