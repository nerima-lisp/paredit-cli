//! `redundant-count-nil`: a remove/delete/substitute call with an explicit :count nil, the default ((remove x seq :count nil) is (remove x seq)).
//!
//! The analysis lives in [`crate::redundant_count_nil::domain`], which also backs the
//! standalone `inspect redundant-count-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_count_nil::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-count-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a remove/delete/substitute call with an explicit :count nil, the default ((remove x seq :count nil) is (remove x seq))",
    Fixability::Fixable,
);

/// Removing/substituting operators whose `:count` keyword defaults to `nil`;
/// mirrors `COUNT_HEADS` in the report module.
const HEADS: [NormalizedHead; 12] = [
    NormalizedHead::new("remove"),
    NormalizedHead::new("remove-if"),
    NormalizedHead::new("remove-if-not"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-if"),
    NormalizedHead::new("delete-if-not"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("substitute-if"),
    NormalizedHead::new("substitute-if-not"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nsubstitute-if"),
    NormalizedHead::new("nsubstitute-if-not"),
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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant :count nil".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} :count defaults to nil; drop the explicit :count nil",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
