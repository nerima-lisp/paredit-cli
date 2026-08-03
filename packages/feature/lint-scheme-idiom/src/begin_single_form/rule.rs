//! `scheme-begin-single-form`: a `begin` that wraps exactly one expression.
//!
//! The analysis lives in [`crate::begin_single_form::domain`], which also backs
//! the standalone `inspect scheme-begin-single-form` command; this module only
//! registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::begin_single_form::domain::{DIALECTS, HEAD, MESSAGE, examine_begin};

pub const META: RuleMeta = RuleMeta::new(
    "scheme-begin-single-form",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a begin wrapping a single expression, which is just that expression",
    Fixability::Fixable,
);

/// `examine_begin` only ever matches a `begin` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new(HEAD)];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut begin_form_count = 0;
        let mut items = Vec::new();
        examine_begin(context.tree(), view, &mut begin_form_count, &mut items);
        for item in items {
            // The inner form is copied from source rather than re-printed, so
            // its spacing, comments and reader prefixes survive the fix.
            let fix = RuleFix::single(
                item.span,
                context.slice(item.inner_span).to_owned(),
                "Unwrap the single-form begin".to_owned(),
            );
            sink.report_fixed(item.span, MESSAGE.to_owned(), fix);
        }
        Ok(())
    }
}
