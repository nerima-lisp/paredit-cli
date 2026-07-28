//! `case-nil-key`: a case/ecase/ccase clause with a bare nil key, which is the empty key list and never matches (use ((nil) …)).
//!
//! The analysis lives in [`crate::case_nil_key::domain`], which also backs the
//! standalone `inspect case-nil-key` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::case_nil_key::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "case-nil-key",
    RuleCategory::DeadCode,
    Severity::Error,
    "a case/ecase/ccase clause with a bare nil key, which is the empty key list and never matches (use ((nil) …))",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("ccase"),
    NormalizedHead::new("ecase"),
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
    ) -> LintResult<()> {
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} clause key nil is the empty key list and never matches; use ((nil) …)",
                    item.head
                ),
            );
        }
        Ok(())
    }
}
