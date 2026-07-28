//! `redundant-identity-key`: a :key-taking call with an explicit :key #'identity/nil, the default ((sort xs #'< :key #'identity) is (sort xs #'<)).
//!
//! The analysis lives in [`crate::redundant_identity_key::domain`], which also backs the
//! standalone `inspect redundant-identity-key` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_identity_key::domain::examine_call;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-identity-key",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a :key-taking call with an explicit :key #'identity/nil, the default ((sort xs #'< :key #'identity) is (sort xs #'<))",
    Fixability::Fixable,
);

/// Operators that accept a `:key` argument; mirrors `KEY_HEADS` in the report
/// module.
const HEADS: [NormalizedHead; 37] = [
    NormalizedHead::new("adjoin"),
    NormalizedHead::new("assoc"),
    NormalizedHead::new("assoc-if"),
    NormalizedHead::new("count"),
    NormalizedHead::new("count-if"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-duplicates"),
    NormalizedHead::new("delete-if"),
    NormalizedHead::new("find"),
    NormalizedHead::new("find-if"),
    NormalizedHead::new("intersection"),
    NormalizedHead::new("member"),
    NormalizedHead::new("member-if"),
    NormalizedHead::new("merge"),
    NormalizedHead::new("mismatch"),
    NormalizedHead::new("nintersection"),
    NormalizedHead::new("nset-difference"),
    NormalizedHead::new("nset-exclusive-or"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nunion"),
    NormalizedHead::new("position"),
    NormalizedHead::new("position-if"),
    NormalizedHead::new("pushnew"),
    NormalizedHead::new("rassoc"),
    NormalizedHead::new("reduce"),
    NormalizedHead::new("remove"),
    NormalizedHead::new("remove-duplicates"),
    NormalizedHead::new("remove-if"),
    NormalizedHead::new("search"),
    NormalizedHead::new("set-difference"),
    NormalizedHead::new("set-exclusive-or"),
    NormalizedHead::new("sort"),
    NormalizedHead::new("stable-sort"),
    NormalizedHead::new("subsetp"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("substitute-if"),
    NormalizedHead::new("union"),
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
        examine_call(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Delete the redundant ` :key #'identity` argument pair.

                RuleFix::multi(
                    "Drop the redundant :key #'identity".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} defaults :key to identity; the explicit :key #'identity is redundant",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
