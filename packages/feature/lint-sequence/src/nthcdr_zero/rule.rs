//! `nthcdr-zero`: an nthcdr with a zero count, which returns the list unchanged ((nthcdr 0 x) is x).
//!
//! The analysis lives in [`crate::nthcdr_zero::domain`], which also backs the
//! standalone `inspect nthcdr-zero` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nthcdr_zero::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nthcdr-zero",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an nthcdr with a zero count, which returns the list unchanged ((nthcdr 0 x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("nthcdr")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut nthcdr_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut nthcdr_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (nthcdr 0 x) is x: replace the whole form with the list source.

                RuleFix::single(
                    item.span,
                    context_slice(item.list_span),
                    "Drop the no-op (nthcdr 0 …)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "nthcdr with a zero count returns the list unchanged; (nthcdr 0 x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
