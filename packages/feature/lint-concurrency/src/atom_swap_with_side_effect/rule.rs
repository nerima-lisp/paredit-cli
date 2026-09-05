//! `atom-swap-with-side-effect`: a retried update function that does more than
//! compute a value.
//!

use paredit_core_lint_engine::LintResult;

use crate::atom_swap_with_side_effect::domain::examine_swap;
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
    "atom-swap-with-side-effect",
    RuleCategory::Concurrency,
    Severity::Warning,
    "a swap!/alter update function with a side effect, which its retries repeat",
    // Moving the effect out of the update function changes where it happens
    // and how often; only the author can decide where it belongs.
    Fixability::ReportOnly,
);

/// `examine_swap` only ever matches these four heads.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("swap!"),
    NormalizedHead::new("swap-vals!"),
    NormalizedHead::new("alter"),
    NormalizedHead::new("commute"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// The trait default is Common Lisp only, which has no `swap!` at all. This
    /// rule encodes Clojure reference semantics and nothing else.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::CLOJURE_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut swap_count = 0;
        let mut items = Vec::new();
        examine_swap(view, &mut swap_count, &mut items);
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
