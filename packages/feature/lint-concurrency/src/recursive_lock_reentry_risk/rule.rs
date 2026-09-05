//! `recursive-lock-reentry-risk`: a non-recursive lock taken again inside its
//! own scope.
//!

use paredit_core_lint_engine::LintResult;

use crate::recursive_lock_reentry_risk::domain::examine_lock_scope;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "recursive-lock-reentry-risk",
    RuleCategory::Concurrency,
    // A heuristic: the nesting is certain, the reachability is not. A warning
    // says "look at this", which is what the finding's own sentence says too.
    Severity::Warning,
    "the same non-recursive lock taken again inside its own scope, a deadlock risk",
    // The two repairs — a recursive lock, or restructuring so the inner form
    // does not take the lock — are different programs.
    Fixability::ReportOnly,
);

/// The non-reentrant lock forms. `with-recursive-lock-held`,
/// `with-recursive-lock` and Clojure's `locking` are absent on purpose: all
/// three may be reentered, so nesting one is not a defect.
const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("with-lock-held"),
    NormalizedHead::new("with-mutex"),
    NormalizedHead::new("with-locked-hash-table"),
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
        let mut lock_form_count = 0;
        let mut items = Vec::new();
        examine_lock_scope(view, &mut lock_form_count, &mut items);
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
