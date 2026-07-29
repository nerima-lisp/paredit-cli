//! `cons-to-list`: a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b)).
//!
//! The analysis lives in [`crate::cons_to_list::domain`], which also backs the
//! standalone `inspect cons-to-list` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::cons_to_list::domain::examine_cons;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cons-to-list",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cons onto nil or a list literal ((cons a nil) is (list a); (cons a (list b)) is (list a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cons")];

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
        let mut cons_form_count = 0;
        let mut items = Vec::new();
        examine_cons(view, context.source(), &mut cons_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite as `(list ELEMENT [TAIL_ELEMENTS])`.
                let element = context_slice(item.element_span);
                let text = match item.tail_elements_span {
                    Some(tail) => format!("(list {} {})", element, context_slice(tail)),
                    None => format!("(list {element})"),
                };

                RuleFix::single(item.span, text, "Rewrite the cons as a list".to_owned())
            };

            sink.report_fixed(
                span,
                "cons onto nil/a list is a list constructor; use list".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
