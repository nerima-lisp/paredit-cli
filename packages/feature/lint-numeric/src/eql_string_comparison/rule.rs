//! `eql-string-comparison`: an eq/eql compared against a string literal (never reliably eql).
//!
//! The analysis lives in [`crate::eql_string_comparison::domain`], which also backs the
//! standalone `inspect eql-string-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::eql_string_comparison::domain::examine_comparison;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eql-string-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "an eq/eql compared against a string literal (never reliably eql)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("eq"), NormalizedHead::new("eql")];

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
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(
            view,
            context.source(),
            &mut comparison_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} compares against string literal {}",
                    item.operator, item.literal
                ),
            );
        }
        Ok(())
    }
}
