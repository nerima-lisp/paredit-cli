//! `butlast-default-count`: a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x)).
//!
//! The analysis lives in [`crate::butlast_default_count::domain`], which also backs the
//! standalone `inspect butlast-default-count` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::butlast_default_count::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "butlast-default-count",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a butlast/nbutlast call with an explicit count of 1, the default ((butlast x 1) is (butlast x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("butlast"),
    NormalizedHead::new("nbutlast"),
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant butlast count of 1".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit count of 1 restates butlast's default; (butlast x 1) is (butlast x)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
