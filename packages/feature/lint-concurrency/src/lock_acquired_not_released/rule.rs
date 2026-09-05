//! `lock-acquired-not-released`: a lock taken by hand with no cleanup to give
//! it back.
//!

use paredit_core_lint_engine::LintResult;

use crate::lock_acquired_not_released::domain::examine_acquire;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "lock-acquired-not-released",
    // A held lock is a resource that leaks on a non-local exit, which is what
    // this category is for and what its sibling `unclosed-stream` flags for
    // file handles. The defect is the missing cleanup, not the concurrency.
    RuleCategory::Resource,
    // Same as `unclosed-stream`: the leak does not degrade the program, it
    // stops it.
    Severity::Error,
    "a manually acquired lock with no unwind-protect to release it on a non-local exit",
    // The right repair is usually `with-lock-held`, which changes the shape of
    // the whole body; where the release belongs is a design decision.
    Fixability::ReportOnly,
);

/// `examine_acquire` only ever matches a manual acquisition. The scoped macros
/// deliberately do not appear: they release by construction, and a rule that
/// never sees them cannot report one.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("acquire-lock"),
    NormalizedHead::new("grab-mutex"),
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
        let mut acquire_count = 0;
        let mut items = Vec::new();
        examine_acquire(context.tree(), view, &mut acquire_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
