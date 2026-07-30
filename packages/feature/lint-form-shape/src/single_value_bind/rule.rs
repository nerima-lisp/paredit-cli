//! `single-value-bind`: a multiple-value-bind of one variable ((multiple-value-bind (x) f body) is (let ((x f)) body)).
//!
//! The analysis lives in [`crate::single_value_bind::domain`], which also backs the
//! standalone `inspect single-value-bind` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::single_value_bind::domain::examine_bind;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "single-value-bind",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multiple-value-bind of one variable ((multiple-value-bind (x) f body) is (let ((x f)) body))",
    Fixability::Fixable,
);

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut bind_form_count = 0;
        let mut items = Vec::new();
        examine_bind(view, &mut bind_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite as a plain let: (multiple-value-bind (x) f body) -> (let ((x f)) body).
                let binding = format!(
                    "({} {})",
                    context_slice(item.var_span),
                    context_slice(item.form_span)
                );
                let text = match item.body_span {
                    Some(body) => format!("(let ({}) {})", binding, context_slice(body)),
                    None => format!("(let ({binding}))"),
                };

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite single-value multiple-value-bind as let".to_owned(),
                )
            };

            sink.report_fixed(span, "multiple-value-bind of one variable is just let; (multiple-value-bind (x) f body) is (let ((x f)) body)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
