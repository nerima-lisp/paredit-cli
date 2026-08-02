//! `when-unless-implicit-nil-misused`: a when/unless value handed to an
//! operator that requires a number.
//!
//! The analysis lives in [`crate::when_unless_implicit_nil_misused::domain`],
//! which also backs the standalone `inspect when-unless-implicit-nil-misused`
//! command; this module only registers it with the lint suite and phrases its
//! findings.
//!
//! The head filter is the *arithmetic* operators rather than `when`/`unless`.
//! That inversion is what makes the check local to the matched node — see the
//! domain module for why anchoring on `when` would need a per-invocation scan.

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::when_unless_implicit_nil_misused::domain::examine_arithmetic;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "when-unless-implicit-nil-misused",
    RuleCategory::Suspicious,
    Severity::Error,
    "a when/unless value used as an argument to an operator that requires a number",
    Fixability::ReportOnly,
);

/// The strict numeric operators, in the same order as
/// [`crate::when_unless_implicit_nil_misused::domain::STRICT_NUMERIC_HEADS`].
/// A head missing here makes the rule unreachable for that operator while every
/// `examine_arithmetic` test still passes, which is what the engine-pass test
/// in `lib.rs` exists to catch.
const HEADS: [NormalizedHead; 21] = [
    NormalizedHead::new("+"),
    NormalizedHead::new("-"),
    NormalizedHead::new("*"),
    NormalizedHead::new("/"),
    NormalizedHead::new("1+"),
    NormalizedHead::new("1-"),
    NormalizedHead::new("mod"),
    NormalizedHead::new("rem"),
    NormalizedHead::new("abs"),
    NormalizedHead::new("signum"),
    NormalizedHead::new("sqrt"),
    NormalizedHead::new("isqrt"),
    NormalizedHead::new("expt"),
    NormalizedHead::new("gcd"),
    NormalizedHead::new("lcm"),
    NormalizedHead::new("max"),
    NormalizedHead::new("min"),
    NormalizedHead::new("floor"),
    NormalizedHead::new("ceiling"),
    NormalizedHead::new("truncate"),
    NormalizedHead::new("round"),
];

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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_arithmetic(view, &mut arithmetic_form_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once for the call rather than once per argument: every item
        // here belongs to this one form, so they share its verdict.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(
                item.span,
                format!(
                    "{} yields nil when its test fails, and {} requires a number",
                    item.conditional, item.operator
                ),
            );
        }
        Ok(())
    }
}
