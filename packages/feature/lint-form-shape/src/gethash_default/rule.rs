//! `gethash-default`: a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h)).
//!
//! The analysis lives in [`crate::gethash_default::domain`], which also backs the
//! standalone `inspect gethash-default` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::gethash_default::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "gethash-default",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("gethash")];

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
        let mut gethash_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut gethash_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Delete the redundant ` nil` default.

                RuleFix::multi(
                    "Drop the redundant nil default".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "the gethash default is nil; (gethash k h nil) is (gethash k h)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
