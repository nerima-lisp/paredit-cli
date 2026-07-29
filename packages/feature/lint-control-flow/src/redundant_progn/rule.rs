//! `redundant-progn`: a progn that is empty or wraps a single form (progn X is just X).
//!
//! The analysis lives in [`crate::redundant_progn::domain`], which also backs the
//! standalone `inspect redundant-progn` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_progn::domain::examine_progn;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a progn that is empty or wraps a single form (progn X is just X)",
    Fixability::Fixable,
);

/// `examine_progn` only ever matches a `progn` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("progn")];

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
        let mut progn_form_count = 0;
        let mut items = Vec::new();
        examine_progn(view, context.source(), &mut progn_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // An empty progn is `nil`; a single-form progn becomes that form,
                // copied verbatim from source to preserve reader prefixes/spacing.
                let replacement = item
                    .inner_span
                    .map_or_else(|| "nil".to_owned(), |span| context.slice(span).to_owned());

                RuleFix::single(
                    item.span,
                    replacement,
                    "Unwrap the redundant progn".to_owned(),
                )
            };
            let detail = if item.body_form_count == 0 {
                "an empty progn is nil".to_owned()
            } else {
                "progn wraps a single form; it is equivalent to that form".to_owned()
            };

            sink.report_fixed(span, format!("redundant progn: {detail}"), fix);
        }
        Ok(())
    }
}
