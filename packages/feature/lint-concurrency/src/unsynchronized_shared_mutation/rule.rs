//! `unsynchronized-shared-mutation`: a global written on a new thread with
//! nothing serializing the write.
//!
//! The analysis lives in [`crate::unsynchronized_shared_mutation::domain`],
//! which also backs the standalone `inspect unsynchronized-shared-mutation`
//! command; this module only registers it with the lint suite and phrases its
//! findings.
//!
//! Expect this rule and `lint-safety`'s `global-mutation-in-function` to fire
//! together on the same line: a thread thunk is a `lambda`, which is one of
//! that rule's heads. Neither is redundant — that one says the name looks like
//! a special variable, this one says the write happens on a new thread with
//! nothing serializing it — and their spans differ (the whole write form there,
//! the variable reference here) so they do not read as one finding printed
//! twice. The domain module's "Relationship to `global-mutation-in-function`"
//! section has the full account.

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::unsynchronized_shared_mutation::domain::examine_spawn;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "unsynchronized-shared-mutation",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a global written inside a thread body with no lock held, which races",
    // A lock, an atomic, or not sharing the cell — the choice is the program's,
    // and no rewrite can make it.
    Fixability::ReportOnly,
);

/// `examine_spawn` only ever matches a `make-thread` head, which is how both
/// `bordeaux-threads` and `sb-thread` spell it.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-thread")];

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
        let mut spawn_count = 0;
        let mut items = Vec::new();
        examine_spawn(view, &mut spawn_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // The dispatcher walks into quoted data like any other subtree, so a
        // `'(make-thread …)` reaches here as a head match. Asked only once a
        // finding already exists, and costs the node's depth.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
