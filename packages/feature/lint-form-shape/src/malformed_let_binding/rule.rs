//! `malformed-let-binding`: a let/let* binding that is neither a symbol nor a (var value) pair.
//!
//! The analysis lives in [`crate::malformed_let_binding::domain`], which also backs the
//! standalone `inspect malformed-let-binding` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::malformed_let_binding::domain::examine_let;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "malformed-let-binding",
    RuleCategory::Malformed,
    Severity::Error,
    "a let/let* binding that is neither a symbol nor a (var value) pair",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("let"), NormalizedHead::new("let*")];

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
        let mut let_form_count = 0;
        let mut items = Vec::new();
        examine_let(view, context.source(), &mut let_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "let binding {} has {} elements; expected a symbol or (var value)",
                    item.binding, item.element_count
                ),
            );
        }
        Ok(())
    }
}
