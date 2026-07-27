//! `eql-search-literal`: a member/assoc/find/substitute/adjoin/... searching for a string/list literal without :test (default eql won't match).
//!
//! The analysis lives in [`crate::eql_search_literal::domain`], which also backs the
//! standalone `inspect eql-search-literal` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::eql_search_literal::domain::examine_call;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eql-search-literal",
    RuleCategory::Suspicious,
    Severity::Error,
    "a member/assoc/find/substitute/adjoin/... searching for a string/list literal without :test (default eql won't match)",
    Fixability::ReportOnly,
);

/// Every default-`eql` search function `examine_call` recognizes: the
/// first-argument searchers plus the second-argument (`old`) substituters.
const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("member"),
    NormalizedHead::new("assoc"),
    NormalizedHead::new("rassoc"),
    NormalizedHead::new("find"),
    NormalizedHead::new("position"),
    NormalizedHead::new("count"),
    NormalizedHead::new("remove"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("adjoin"),
    NormalizedHead::new("pushnew"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("subst"),
    NormalizedHead::new("nsubst"),
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
    ) -> Result<()> {
        let mut search_call_count = 0;
        let mut items = Vec::new();
        examine_call(view, context.path(), &mut search_call_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} searches for literal {} with the default eql test; add :test",
                    item.operator, item.literal
                ),
            );
        }
        Ok(())
    }
}
