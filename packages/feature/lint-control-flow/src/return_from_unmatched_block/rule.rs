//! `return-from-unmatched-block`: a `return-from` naming a block that does not
//! lexically enclose it.
//!
//! The analysis lives in [`crate::return_from_unmatched_block::domain`], which
//! also backs the standalone `inspect return-from-unmatched-block` command;
//! this module only registers it with the lint suite and phrases its findings.
//!
//! `ReportOnly`, and not because a fix is hard to write: every repair —
//! renaming the block, moving the exit, deleting it — changes which form the
//! program returns from. Rewriting control flow is not a mechanical edit.
//!
//! # Cost
//!
//! `Heads(["return-from"])`, so a file with no `return-from` pays one hash
//! lookup per list node and nothing else — which is what the `clean/forms/*`
//! benchmarks measure. Once matched, the ancestor walk materializes only the
//! *one* enclosing top-level form (`crate::support::with_lexical_chain`), so N
//! exits in a file cost N × their own definitions, never N × the file.

use paredit_core_lint_engine::LintResult;

use crate::return_from_unmatched_block::domain::{examine_return_from, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "return-from-unmatched-block",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a return-from naming a block that does not lexically enclose it",
    Fixability::ReportOnly,
);

/// `examine_return_from` only ever matches a `return-from` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("return-from")];

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
        let mut return_from_form_count = 0;
        let mut items = Vec::new();
        examine_return_from(
            context.tree(),
            view,
            &mut return_from_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, message_for(&item.block_name));
        }
        Ok(())
    }
}
