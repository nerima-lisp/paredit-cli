//! `redundant-prog1`: a prog1 wrapping a single form, which is just that form ((prog1 x) is x).
//!
//! The analysis lives in [`crate::redundant_prog1::domain`], which also backs the
//! standalone `inspect redundant-prog1` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_prog1::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-prog1",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a prog1 wrapping a single form, which is just that form ((prog1 x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("prog1")];

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
        let mut prog1_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut prog1_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (prog1 x) is x: replace the whole form with its single inner form.

                RuleFix::single(
                    item.span,
                    context_slice(item.form_span),
                    "Drop the single-form prog1".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a prog1 wrapping a single form is just that form; (prog1 x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
