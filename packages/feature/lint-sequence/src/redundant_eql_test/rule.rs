//! `redundant-eql-test`: an eql-defaulting call with an explicit :test #'eql, the default ((find x l :test #'eql) is (find x l)).
//!
//! The analysis lives in [`crate::redundant_eql_test::domain`], which also backs the
//! standalone `inspect redundant-eql-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_eql_test::domain::examine_call;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-eql-test",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an eql-defaulting call with an explicit :test #'eql, the default ((find x l :test #'eql) is (find x l))",
    Fixability::Fixable,
);

/// Operators whose `:test` defaults to `eql`; mirrors `EQL_TEST_HEADS` in the
/// report module.
const HEADS: [NormalizedHead; 30] = [
    NormalizedHead::new("adjoin"),
    NormalizedHead::new("assoc"),
    NormalizedHead::new("count"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-duplicates"),
    NormalizedHead::new("find"),
    NormalizedHead::new("intersection"),
    NormalizedHead::new("member"),
    NormalizedHead::new("mismatch"),
    NormalizedHead::new("nintersection"),
    NormalizedHead::new("nset-difference"),
    NormalizedHead::new("nset-exclusive-or"),
    NormalizedHead::new("nsublis"),
    NormalizedHead::new("nsubst"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nunion"),
    NormalizedHead::new("position"),
    NormalizedHead::new("pushnew"),
    NormalizedHead::new("rassoc"),
    NormalizedHead::new("remove"),
    NormalizedHead::new("remove-duplicates"),
    NormalizedHead::new("search"),
    NormalizedHead::new("set-difference"),
    NormalizedHead::new("set-exclusive-or"),
    NormalizedHead::new("sublis"),
    NormalizedHead::new("subsetp"),
    NormalizedHead::new("subst"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("tree-equal"),
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine_call(view, &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Delete the redundant ` :test #'eql` argument pair.

                RuleFix::multi(
                    "Drop the redundant :test #'eql".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} defaults :test to eql; the explicit :test #'eql is redundant",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
