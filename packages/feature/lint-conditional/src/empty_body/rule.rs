//! `empty-body`: a when/unless/dolist/dotimes with no body (the test/spec runs, then nothing happens).
//!
//! The analysis lives in [`crate::empty_body::domain`], which also backs the
//! standalone `inspect empty-body` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::empty_body::domain::examine_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "empty-body",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a when/unless/dolist/dotimes with no body (the test/spec runs, then nothing happens)",
    Fixability::ReportOnly,
);

/// The four body-requiring forms `examine_form` recognizes.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
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
        let mut body_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, context.path(), &mut body_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} has no body; the test/spec runs, then nothing",
                    item.head
                ),
            );
        }
        Ok(())
    }
}
