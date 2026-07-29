//! `last-default-count`: a last call with an explicit count of 1, the default ((last x 1) is (last x)).
//!
//! The analysis lives in [`crate::last_default_count::domain`], which also backs the
//! standalone `inspect last-default-count` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::last_default_count::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "last-default-count",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a last call with an explicit count of 1, the default ((last x 1) is (last x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("last")];

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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant last count of 1".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit count of 1 restates last's default; (last x 1) is (last x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
