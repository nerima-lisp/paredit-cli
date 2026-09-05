//! `disabled-test-left-in`: a committed test that can never run.
//!

use paredit_core_lint_engine::LintResult;

use crate::disabled_test_left_in::domain::examine_test;
use crate::support::{TEST_DEFINITION_HEADS, TEST_DIALECTS, is_unevaluated_at};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `DeadCode` — "code that can never run" is literally what an unconditionally
/// skipped test is, and it is the only category here that says so.
///
/// `Warning` rather than `Error`: skipping a test while a fix is in flight is a
/// legitimate thing to do for a while. What this rule is for is making sure
/// "for a while" is visible.
pub const META: RuleMeta = RuleMeta::new(
    "disabled-test-left-in",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a test disabled in place rather than removed",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// Anchored on the test definition. The Clojure half of this rule reads
    /// metadata that is a *sibling* of the test's name inside the definition
    /// form, so there is no inner call to anchor on at all.
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
        for item in items {
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let message = format!(
                "test {} is disabled in place by {}; run it again or delete it",
                item.test_name, item.marker
            );
            sink.report(span, message);
        }
        Ok(())
    }
}
