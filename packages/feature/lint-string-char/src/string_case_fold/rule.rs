//! `string-case-fold`: a string= of two same-case-folded operands ((string= (string-downcase a) (string-downcase b)) is (string-equal a b)).
//!
//! The analysis lives in [`crate::string_case_fold::domain`], which also backs the
//! standalone `inspect string-case-fold` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::string_case_fold::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "string-case-fold",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a string= of two same-case-folded operands ((string= (string-downcase a) (string-downcase b)) is (string-equal a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("string=")];

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
        let mut compare_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut compare_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (string= (string-downcase a) (string-downcase b)) is (string-equal a b).
                let text = format!(
                    "(string-equal {} {})",
                    context_slice(item.left_span),
                    context_slice(item.right_span)
                );

                RuleFix::single(item.span, text, "Rewrite as (string-equal a b)".to_owned())
            };

            sink.report_fixed(
                span,
                "case-folding both sides of string= is case-insensitive; use string-equal"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
