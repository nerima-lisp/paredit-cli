//! `racket-match-unreachable-clause`: a `match` clause an earlier catch-all
//! makes dead.
//!
//! The analysis lives in [`crate::match_unreachable_clause::domain`]; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::match_unreachable_clause::domain::{DIALECTS, examine_match};

pub const META: RuleMeta = RuleMeta::new(
    "racket-match-unreachable-clause",
    RuleCategory::DeadCode,
    Severity::Error,
    "a match clause an earlier catch-all pattern makes unreachable",
    Fixability::ReportOnly,
);

/// Exactly the heads [`examine_match`] can match, kept in step with
/// `domain::HEADS` by the test below.
const FILTER_HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("match"),
    NormalizedHead::new("match-lambda"),
    NormalizedHead::new("match-lambda*"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&FILTER_HEADS)
    }

    /// Reads `domain::DIALECTS`, the same constant the report's
    /// `dialect_modelled` flag reads, so scope and dialect gate cannot drift.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut match_form_count = 0;
        let mut items = Vec::new();
        examine_match(context.tree(), view, &mut match_form_count, &mut items);
        for item in items {
            sink.report(item.span, paredit_core_cli::report::Finding::message(&item));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_unreachable_clause::domain::HEADS;

    /// The `HeadFilter` is a pre-filter: an over-approximation only costs a
    /// wasted call, but an under-approximation silently drops every finding at
    /// a head the domain can match. Pinning the two together makes adding a
    /// head to one and not the other a test failure rather than a silent hole.
    #[test]
    fn the_filter_heads_are_exactly_the_domain_heads() {
        let filter: Vec<&str> = FILTER_HEADS.iter().map(|head| head.as_str()).collect();
        assert_eq!(filter, HEADS.to_vec());
    }

    /// `NormalizedHead::new` asserts at compile time, but the domain's own
    /// spellings are plain `&str` and could drift into a shape the index can
    /// never produce.
    #[test]
    fn every_domain_head_is_index_normalized() {
        for head in HEADS {
            assert!(!head.is_empty());
            assert!(!head.contains(':'), "{head} would never reach the index");
            assert_eq!(head, head.to_ascii_lowercase(), "{head} is not normalized");
        }
    }
}
