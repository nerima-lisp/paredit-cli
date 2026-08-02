//! `test-asserts-constant`: a test assertion whose truth is settled by the
//! source.
//!
//! The analysis lives in [`crate::test_asserts_constant::domain`], which also
//! backs the standalone `inspect test-asserts-constant` command; this module
//! only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::support::{TEST_DEFINITION_HEADS, TEST_DIALECTS, is_unevaluated_at};
use crate::test_asserts_constant::domain::examine_test;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// `Suspicious`, following `self-comparison` — the nearest existing rule, and
/// the one whose findings this deliberately does not overlap.
///
/// `Warning` rather than `self-comparison`'s `Error`, because `(is true)` is
/// also how a placeholder or a "we got this far" smoke check is written, and
/// this rule cannot tell a deliberate one from an abandoned one.
pub const META: RuleMeta = RuleMeta::new(
    "test-asserts-constant",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a test assertion on a constant that can never fail",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// Anchored on the test definition rather than on `is`/`should`, so that
    /// the rule never fires on a project's own function named `is` outside a
    /// test — and so that a finding can name the test it belongs to.
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
        let mut assertion_form_count = 0;
        let mut items = Vec::new();
        examine_test(
            view,
            context.dialect(),
            &mut assertion_form_count,
            &mut items,
        );
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = format!(
                "{} in test {} can never fail: {}",
                item.assertion,
                item.test_name,
                item.shape.detail()
            );
            sink.report(span, message);
        }
        Ok(())
    }
}
