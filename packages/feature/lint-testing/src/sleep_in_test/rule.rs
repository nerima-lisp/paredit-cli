//! `sleep-in-test`: a wall-clock sleep inside a test body.
//!
//! The analysis lives in [`crate::sleep_in_test::domain`], which also backs the
//! standalone `inspect sleep-in-test` command; this module only registers it
//! with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::sleep_in_test::domain::examine_test;
use crate::support::{TEST_DEFINITION_HEADS, TEST_DIALECTS, is_unevaluated_at};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Suspicious`: the call is well-formed and does exactly what it says, and
/// what is wrong is that the test's result now depends on something the source
/// does not mention.
///
/// Not `Performance` — the cost is real but constant, and this rule exists for
/// the flakiness rather than the seconds. Not `Concurrency` either: a sleep is
/// often a stand-in for synchronization, but plenty of them wait on a timer
/// with no second thread in sight.
pub const META: RuleMeta = RuleMeta::new(
    "sleep-in-test",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a sleep-family call inside a test body",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// Anchored on the test definition, not on `sleep`. A filter over the sleep
    /// heads would match every sleep in the program and then have to work out
    /// whether it is inside a test, which is ancestor context a per-node
    /// predicate does not have.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&TEST_DEFINITION_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&TEST_DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut test_form_count = 0;
        let mut items = Vec::new();
        examine_test(view, context.dialect(), &mut test_form_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once per test that has a finding, not once per sleep and never
        // per visited node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = format!(
                "{} inside test {}; the test's outcome depends on wall-clock timing",
                item.head, item.test_name
            );
            sink.report(span, message);
        }
        Ok(())
    }
}
