//! `nested-string-case`: nested string case ops where the outer dominates ((string-upcase (string-downcase s)) is (string-upcase s)).
//!
//! The analysis lives in [`crate::nested_string_case::domain`], which also backs the
//! standalone `inspect nested-string-case` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nested_string_case::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-string-case",
    RuleCategory::Suspicious,
    Severity::Warning,
    "nested string case ops where the outer dominates ((string-upcase (string-downcase s)) is (string-upcase s))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("string-upcase"),
    NormalizedHead::new("string-downcase"),
    NormalizedHead::new("string-capitalize"),
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
        let context_slice = |span| context.slice(span).to_owned();
        let mut string_case_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.source(),
            &mut string_case_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // (OUTER (INNER s)) is (OUTER s), keeping the outer op.
                let text = format!(
                    "({} {})",
                    context_slice(item.outer_span),
                    context_slice(item.string_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Collapse the nested string case op".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "the outer string case op dominates; the inner one is dead work".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
