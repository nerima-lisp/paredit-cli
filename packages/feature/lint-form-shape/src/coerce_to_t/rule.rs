//! `coerce-to-t`: a coerce to type t, which returns the object unchanged ((coerce x t) is x).
//!
//! The analysis lives in [`crate::coerce_to_t::domain`], which also backs the
//! standalone `inspect coerce-to-t` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::coerce_to_t::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "coerce-to-t",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a coerce to type t, which returns the object unchanged ((coerce x t) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("coerce")];

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
        let mut coerce_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.source(), &mut coerce_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (coerce x t) is x.

                RuleFix::single(
                    item.span,
                    context_slice(item.object_span),
                    "Drop the no-op (coerce x t)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "coerce to type t returns the object unchanged; (coerce x t) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
