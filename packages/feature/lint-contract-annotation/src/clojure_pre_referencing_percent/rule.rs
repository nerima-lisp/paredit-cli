//! `clojure-pre-referencing-percent`: a `:pre` condition naming `%`, which only
//! binds inside `:post`.
//!
//! The analysis lives in
//! [`crate::clojure_pre_referencing_percent::domain`], which also backs the
//! standalone report; this module only registers it with the lint suite and
//! phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::clojure_pre_referencing_percent::domain::{SCOPE, examine_defn, is_data_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "clojure-pre-referencing-percent",
    // The condition is well-formed and says something the author did not mean:
    // `%` is the return value, and a precondition runs before there is one.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a defn :pre condition naming %, which clojure.core's fn binds only inside :post",
    // Whether the condition belongs in :post or should have named a parameter
    // is the author's call, and the two repairs are different edits.
    Fixability::ReportOnly,
);

/// `examine_defn` only ever matches these two heads.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("defn"), NormalizedHead::new("defn-")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Read from the same constant the standalone report's `dialect_modelled`
    /// flag uses, so the two cannot drift.
    fn dialect_scope(&self) -> RuleDialectScope {
        SCOPE
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut pre_condition_count = 0;
        let mut items = Vec::new();
        examine_defn(view, &mut pre_condition_count, &mut items);
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
