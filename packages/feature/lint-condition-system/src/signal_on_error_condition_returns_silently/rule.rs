//! `signal-on-error-condition-returns-silently`: an error signalled the way a
//! notification is.
//!
//! The analysis lives in
//! [`crate::signal_on_error_condition_returns_silently::domain`], which also
//! backs the standalone `inspect signal-on-error-condition-returns-silently`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::signal_on_error_condition_returns_silently::domain::examine_signal;
use crate::support::{LazyHierarchy, is_unevaluated_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "signal-on-error-condition-returns-silently",
    RuleCategory::Conditions,
    Severity::Warning,
    "a signal of a same-file error subtype, which returns nil instead of entering the debugger",
    // Rewriting `signal` to `error` changes what happens when the condition
    // *is* handled and the handler declines, so it is not a value-preserving
    // edit.
    Fixability::ReportOnly,
);

/// `examine_signal` only ever matches a `signal` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("signal")];

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
        // Costs nothing to build; reads the file only if `examine_signal` gets
        // as far as a literal condition type.
        let hierarchy = LazyHierarchy::new(context.tree());
        let mut signal_call_count = 0;
        let mut items = Vec::new();
        examine_signal(view, &hierarchy, &mut signal_call_count, &mut items);
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
