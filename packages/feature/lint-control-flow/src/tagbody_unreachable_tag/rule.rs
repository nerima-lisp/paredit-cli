//! `tagbody-unreachable-tag`: a tagbody label no `go` in the form ever names.
//!
//!
//! `ReportOnly` even though deleting a dead label looks safe: this rule cannot
//! see macro expansions, and `paredit-feature-remove-unused`'s
//! `remove-unused-tag` command is where a caller-directed rewrite already
//! lives.
//!
//! # Cost
//!
//! `Heads(["tagbody"])`, and everything it then reads is the matched form's
//! own subtree, so a file of T tagbodies costs the file once — not T times.

use paredit_core_lint_engine::LintResult;

use crate::tagbody_unreachable_tag::domain::{examine_tagbody, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "tagbody-unreachable-tag",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a tagbody label no go in the form ever targets",
    Fixability::ReportOnly,
);

/// `examine_tagbody` only ever matches a `tagbody` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("tagbody")];

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
        let mut tagbody_form_count = 0;
        let mut items = Vec::new();
        examine_tagbody(context.tree(), view, &mut tagbody_form_count, &mut items);
        for item in items {
            sink.report(item.span, message_for(&item.tag));
        }
        Ok(())
    }
}
