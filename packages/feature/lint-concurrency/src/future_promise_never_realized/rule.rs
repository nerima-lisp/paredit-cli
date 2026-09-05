//! `future-promise-never-realized`: a deferred value bound and then never
//! mentioned.
//!

use paredit_core_lint_engine::LintResult;

use crate::future_promise_never_realized::domain::examine_binding;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "future-promise-never-realized",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a future or promise bound by let and never mentioned, discarding its value and errors",
    // Whether the right repair is a deref, a different place to await, or
    // deleting the binding depends on what the value was for.
    Fixability::ReportOnly,
);

/// The binding forms whose body could hold a reference. Narrower than "every
/// list": a rule keyed on `future` itself would have to walk *up* to find the
/// binding, which `RuleContext` cannot do cheaply.
const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("let"),
    NormalizedHead::new("loop"),
    NormalizedHead::new("if-let"),
    NormalizedHead::new("when-let"),
    NormalizedHead::new("binding"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Futures and promises are Clojure reference types; Common Lisp has
    /// neither, and its `let` binds with a list rather than a vector.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut binding_count = 0;
        let mut items = Vec::new();
        examine_binding(view, &mut binding_count, &mut items);
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
