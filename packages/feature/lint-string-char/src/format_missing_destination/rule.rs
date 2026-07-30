//! `format-missing-destination`: a format call whose first argument is a string literal (nil/t/stream destination is missing).
//!
//! The analysis lives in [`crate::format_missing_destination::domain`], which also backs the
//! standalone `inspect format-missing-destination` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::format_missing_destination::domain::examine_format;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "format-missing-destination",
    RuleCategory::Suspicious,
    Severity::Error,
    "a format call whose first argument is a string literal (nil/t/stream destination is missing)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("format")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut format_call_count = 0;
        let mut items = Vec::new();
        examine_format(view, &mut format_call_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(span, format!(
                            "format destination is the string literal {}; a nil/t/stream destination is missing",
                            item.literal
                        ));
        }
        Ok(())
    }
}
