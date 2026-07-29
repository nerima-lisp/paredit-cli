//! `destructive-literal`: a destructive call (nreverse/sort/delete/nsubstitute/nconc/...) on a quoted list literal (undefined behavior).
//!
//! The analysis lives in [`crate::destructive_literal::domain`], which also backs the
//! standalone `inspect destructive-literal` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::destructive_literal::domain::examine_call;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "destructive-literal",
    RuleCategory::Suspicious,
    Severity::Error,
    "a destructive call (nreverse/sort/delete/nsubstitute/nconc/...) on a quoted list literal (undefined behavior)",
    Fixability::ReportOnly,
);

/// Every destructive function `examine_call` recognizes as covered by
/// [`crate::destructive_literal::domain::sequence_indices`].
const HEADS: [NormalizedHead; 23] = [
    NormalizedHead::new("nreverse"),
    NormalizedHead::new("nreconc"),
    NormalizedHead::new("sort"),
    NormalizedHead::new("stable-sort"),
    NormalizedHead::new("nbutlast"),
    NormalizedHead::new("delete-duplicates"),
    NormalizedHead::new("rplaca"),
    NormalizedHead::new("rplacd"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-if"),
    NormalizedHead::new("delete-if-not"),
    NormalizedHead::new("nsublis"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nsubstitute-if"),
    NormalizedHead::new("nsubstitute-if-not"),
    NormalizedHead::new("nsubst"),
    NormalizedHead::new("nsubst-if"),
    NormalizedHead::new("nsubst-if-not"),
    NormalizedHead::new("nunion"),
    NormalizedHead::new("nintersection"),
    NormalizedHead::new("nset-difference"),
    NormalizedHead::new("nset-exclusive-or"),
    NormalizedHead::new("nconc"),
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
        let mut destructive_call_count = 0;
        let mut items = Vec::new();
        examine_call(
            view,
            context.source(),
            &mut destructive_call_count,
            &mut items,
        );
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} destructively modifies quoted literal {} (undefined behavior)",
                    item.operator, item.literal
                ),
            );
        }
        Ok(())
    }
}
