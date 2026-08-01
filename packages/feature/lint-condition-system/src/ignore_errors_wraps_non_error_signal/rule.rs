//! `ignore-errors-wraps-non-error-signal`: a guard that guards nothing.
//!
//! The analysis lives in
//! [`crate::ignore_errors_wraps_non_error_signal::domain`], which also backs the
//! standalone `inspect ignore-errors-wraps-non-error-signal` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::ignore_errors_wraps_non_error_signal::domain::examine_ignore_errors;
use crate::support::{LazyHierarchy, is_unevaluated_at};
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "ignore-errors-wraps-non-error-signal",
    RuleCategory::Conditions,
    Severity::Warning,
    "an ignore-errors around a signal of a same-file non-error condition it cannot catch",
    // Either the wrapper or the condition's supertype is wrong, and which one
    // is a design question.
    Fixability::ReportOnly,
);

/// `examine_ignore_errors` only ever matches an `ignore-errors` head; the
/// `signal` calls it reports are found by searching that form's own body, not
/// by matching `signal` separately.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("ignore-errors")];

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
        let hierarchy = LazyHierarchy::new(context.tree());
        let mut ignore_errors_form_count = 0;
        let mut items = Vec::new();
        examine_ignore_errors(view, &hierarchy, &mut ignore_errors_form_count, &mut items);
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
