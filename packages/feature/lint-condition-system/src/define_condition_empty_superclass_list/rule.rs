//! `define-condition-empty-superclass-list`: a condition that is not an error.
//!
//! The analysis lives in
//! [`crate::define_condition_empty_superclass_list::domain`], which also backs
//! the standalone `inspect define-condition-empty-superclass-list` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::define_condition_empty_superclass_list::domain::examine_define_condition;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "define-condition-empty-superclass-list",
    RuleCategory::Conditions,
    Severity::Warning,
    "a define-condition with an empty () supertype list, which defaults to condition, not error",
    // `error` and `condition` are both plausible repairs and they mean opposite
    // things. Choosing one for the author would be a guess with teeth.
    Fixability::ReportOnly,
);

/// `examine_define_condition` only ever matches a `define-condition` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("define-condition")];

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
        let mut define_condition_form_count = 0;
        let mut items = Vec::new();
        examine_define_condition(view, &mut define_condition_form_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
