//! `multiple-value-bind-all-ignored`: a `multiple-value-bind` whose body
//! references none of the variables it binds.
//!
//! The analysis lives in
//! [`crate::multiple_value_bind_all_ignored::domain`], which also backs the
//! standalone `inspect multiple-value-bind-all-ignored` command; this module
//! only registers it with the lint suite and phrases its findings.
//!
//! `ReportOnly`: the two repairs — unwrapping to the value form plus a
//! `progn`, or adding `(declare (ignore …))` — mean different things, and
//! `multiple-value-bind` is not value-transparent (it discards extra values
//! its body does not name), so neither is a mechanical rewrite.
//!
//! # Cost
//!
//! `Heads(["multiple-value-bind"])`, and everything it reads is the matched
//! form's own subtree.

use paredit_core_lint_engine::LintResult;

use crate::multiple_value_bind_all_ignored::domain::{examine_multiple_value_bind, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "multiple-value-bind-all-ignored",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a multiple-value-bind none of whose variables the body references",
    Fixability::ReportOnly,
);

/// `examine_multiple_value_bind` only ever matches a `multiple-value-bind`
/// head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("multiple-value-bind")];

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
        let mut multiple_value_bind_form_count = 0;
        let mut items = Vec::new();
        examine_multiple_value_bind(
            context.tree(),
            view,
            &mut multiple_value_bind_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, message_for(&item.variables));
        }
        Ok(())
    }
}
