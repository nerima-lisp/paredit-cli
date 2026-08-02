//! `thread-spawned-without-error-handler`: a thread body that inlines work with
//! no handler, so its errors never reach anyone.
//!
//! The analysis lives in
//! [`crate::thread_spawned_without_error_handler::domain`], which also backs
//! the standalone `inspect thread-spawned-without-error-handler` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::thread_spawned_without_error_handler::domain::examine_spawn;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "thread-spawned-without-error-handler",
    // `Conditions` is the near miss, and the description below — "an error in
    // it is lost" — is almost that category's own wording. It is still the
    // wrong category: the `Conditions` rules are about a *handler* that is
    // written wrongly (a `handler-case` that swallows, a clause that cannot
    // run), and this rule fires where no handler is written at all. What makes
    // the absence a defect is the thread boundary and only the thread boundary
    // — the same body inlined at the call site signals to its caller, and
    // nothing here would be worth saying. So the subject is `make-thread`, not
    // the condition system.
    RuleCategory::Concurrency,
    Severity::Warning,
    "a thread body that inlines work with no handler, so an error in it is lost",
    // Which conditions to catch and what to do about them is the whole of the
    // decision; a generated `handler-case` would be a guess at both.
    Fixability::ReportOnly,
);

/// `examine_spawn` only ever matches a `make-thread` head. Clojure's `future`
/// was here and was removed: dereferencing a future rethrows its exception, so
/// a handler-less future body is not a defect. See the domain module.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-thread")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    // The trait default, `COMMON_LISP_ONLY`, is exactly right here and is left
    // unstated for that reason.

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
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
