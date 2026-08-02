//! `block-name-shadows-outer-block`: a nested `block` reusing an enclosing
//! block's name, so an inner `return-from` exits the inner one.
//!
//! The analysis lives in [`crate::block_name_shadows_outer_block::domain`],
//! which also backs the standalone `inspect block-name-shadows-outer-block`
//! command; this module only registers it with the lint suite and phrases its
//! findings.
//!
//! `ReportOnly`: renaming either block changes which form every
//! `return-from` under it exits, and only the author knows which one each
//! meant.
//!
//! # Cost
//!
//! `Heads(["block"])`, and everything it reads is the matched form's own
//! subtree. The walk stops at the first same-named nested block, so deep
//! nesting is linear in the form rather than quadratic in its depth.

use paredit_core_lint_engine::LintResult;

use crate::block_name_shadows_outer_block::domain::{examine_block, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "block-name-shadows-outer-block",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a nested block reusing an outer block's name",
    Fixability::ReportOnly,
);

/// `examine_block` only ever matches a `block` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("block")];

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
        let mut block_form_count = 0;
        let mut items = Vec::new();
        examine_block(context.tree(), view, &mut block_form_count, &mut items);
        for item in items {
            sink.report(item.span, message_for(&item.block_name));
        }
        Ok(())
    }
}
