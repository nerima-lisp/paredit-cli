//! `check-type-redundant-with-declare`: a runtime type assertion on a variable
//! an adjacent `declare` has already promised.
//!
//! The analysis lives in
//! [`crate::check_type_redundant_with_declare::domain`], which also backs the
//! standalone report; this module only registers it with the lint suite and
//! phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::check_type_redundant_with_declare::domain::{SCOPE, examine_check_type, is_data_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "check-type-redundant-with-declare",
    // A `declare` that contradicts the body around it — here, a body that does
    // not believe its own declaration.
    RuleCategory::Declaration,
    Severity::Warning,
    "a check-type restating the type an adjacent declare already promised for that variable",
    // Whether the declaration or the check should go is the author's call.
    Fixability::ReportOnly,
);

/// `examine_check_type` only ever matches this head. Anchoring on `declare`
/// instead would match every declaration in every file, the overwhelming
/// majority of which have no `check-type` anywhere near them.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("check-type")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Read from the same constant the standalone report's `dialect_modelled`
    /// flag uses. This is the trait default's value, but it is stated rather
    /// than inherited so that the report and the engine share one source.
    fn dialect_scope(&self) -> RuleDialectScope {
        SCOPE
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut check_type_count = 0;
        let mut items = Vec::new();
        examine_check_type(context.tree(), view, &mut check_type_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once per candidate, after the head has matched and a finding is
        // already in hand.
        if is_data_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
