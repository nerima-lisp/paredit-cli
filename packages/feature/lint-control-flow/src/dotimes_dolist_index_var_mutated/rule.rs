//! `dotimes-dolist-index-var-mutated`: the iteration variable of a `dotimes`
//! or `dolist` assigned inside the body.
//!
//!
//! `Portability` rather than `Suspicious`: what the `dotimes` half reports is
//! that CLHS leaves the binding strategy — and so the assignment's effect on
//! the iteration — implementation-dependent, which is the definition of that
//! category. The `dolist` half reports a discarded assignment under the same
//! heading because it is the same mistake about the same variable.
//!
//! `ReportOnly`: every repair (a `let`, a different variable, deleting the
//! assignment) changes what the body computes.
//!
//! # Cost
//!
//! `Heads(["dotimes", "dolist"])`, and everything it reads is the matched
//! form's own subtree.

use paredit_core_lint_engine::LintResult;

use crate::dotimes_dolist_index_var_mutated::domain::{examine_iteration, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "dotimes-dolist-index-var-mutated",
    RuleCategory::Portability,
    Severity::Warning,
    "a dotimes or dolist iteration variable assigned inside the body",
    Fixability::ReportOnly,
);

/// `examine_iteration` only ever matches a `dotimes` or `dolist` head.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("dolist"),
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
        let mut iteration_form_count = 0;
        let mut items = Vec::new();
        examine_iteration(context.tree(), view, &mut iteration_form_count, &mut items);
        for item in items {
            sink.report(item.span, message_for(&item.variable, item.form));
        }
        Ok(())
    }
}
