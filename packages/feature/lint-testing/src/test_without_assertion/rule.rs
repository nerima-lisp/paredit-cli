//! `test-without-assertion`: a test definition whose body never asserts.
//!

use paredit_core_lint_engine::LintResult;

use crate::support::{TEST_DEFINITION_HEADS, TEST_DIALECTS, is_unevaluated_at};
use crate::test_without_assertion::domain::examine_test;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Suspicious` rather than `DeadCode`: the body is reached and does run. What
/// is wrong is that its outcome decides nothing, which is "well-formed code
/// whose meaning is probably not what was intended" exactly.
///
/// `Warning` rather than `Error` because a test that only exercises a code path
/// for its side effects — checking that a call does not signal — is a
/// legitimate thing to write, and this rule cannot tell that apart from an
/// unfinished one.
pub const META: RuleMeta = RuleMeta::new(
    "test-without-assertion",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a test definition whose body contains no assertion form",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// Anchored on the *definition*, never on the assertion. A filter over
    /// assertion heads would see only the tests that already assert, which is
    /// the opposite of what this rule is looking for.
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
            // Only now, with a finding in hand, is it worth walking down from
            // the root to ask whether this `deftest` is code or the body of a
            // macro template.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let message = format!(
                "test {} contains no assertion; it passes whenever it does not signal",
                item.test_name
            );
            sink.report(span, message);
        }
        Ok(())
    }
}
