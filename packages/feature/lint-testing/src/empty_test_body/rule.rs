//! `empty-test-body`: a test definition with no body at all.
//!
//! The analysis lives in [`crate::empty_test_body::domain`], which also backs
//! the standalone `inspect empty-test-body` command; this module only registers
//! it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::empty_test_body::domain::examine_test;
use crate::support::{TEST_DEFINITION_HEADS, TEST_DIALECTS, is_unevaluated_at};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Suspicious`, matching `lint-conditional`'s `empty-body` — the two are the
/// same defect at two scales, and a reader who has seen one should not have to
/// learn a second category for the other.
///
/// Not `Malformed`: every framework here accepts an empty body and reports it
/// as a pass, so the form's *shape* is fine and only its meaning is wrong.
pub const META: RuleMeta = RuleMeta::new(
    "empty-test-body",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a test definition with an empty body",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
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
                "test {} has an empty body; it is reported as a pass having checked nothing",
                item.test_name
            );
            sink.report(span, message);
        }
        Ok(())
    }
}
