//! `destructuring-bind-unused-whole`: a destructuring-bind whose &whole variable is never referenced.
//!
//! The analysis lives in [`crate::destructuring_bind_unused_whole::domain`],
//! which also backs the standalone `inspect destructuring-bind-unused-whole`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::destructuring_bind_unused_whole::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "destructuring-bind-unused-whole",
    // A binding whose value is computed and then discarded: `DeadCode`'s
    // "code that can never run or whose result is discarded".
    RuleCategory::DeadCode,
    Severity::Warning,
    "a destructuring-bind that binds a &whole variable and never references it",
    Fixability::ReportOnly,
);

/// `destructuring-bind` alone. The macro-lambda-list half of the same idea is
/// already covered by `inspect unused-parameters` — see the domain module.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("destructuring-bind")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first. [`examine`] reads the matched node's lambda
    /// list before anything else, and its reference scan — the only part
    /// proportional to the form's size — runs only for a lambda list that
    /// actually opens with a nameable `&whole`. The quote descent runs last,
    /// once, and only for a form already known to be unreferenced.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut whole_binding_count = 0;
        let mut items = Vec::new();
        examine(view, &mut whole_binding_count, &mut items);
        if items.is_empty() || is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = paredit_core_cli::report::Finding::message(&item);
            sink.report(span, message);
        }
        Ok(())
    }
}
