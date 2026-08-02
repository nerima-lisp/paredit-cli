//! `with-open-file-redundant-direction-default`: an explicit :direction :input, which is already open's default.
//!
//! The analysis lives in
//! [`crate::with_open_file_redundant_direction_default::domain`], which also
//! backs the standalone `inspect with-open-file-redundant-direction-default`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::with_open_file_redundant_direction_default::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "with-open-file-redundant-direction-default",
    // The same reading the four sibling default-keyword rules get: the code is
    // correct and says something the standard already says.
    RuleCategory::Suspicious,
    Severity::Warning,
    "an open or with-open-file with an explicit :direction :input, which is already the default ((open p :direction :input) is (open p))",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("open"),
    NormalizedHead::new("with-open-file"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first: [`examine`] reads only the matched node's own
    /// keyword slots, and the quote descent runs only once a redundant pair has
    /// actually been found.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut call_form_count, &mut items);
        if items.is_empty() || is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = paredit_core_cli::report::Finding::message(&item);
            sink.report(span, message);
        }
        Ok(())
    }
}
