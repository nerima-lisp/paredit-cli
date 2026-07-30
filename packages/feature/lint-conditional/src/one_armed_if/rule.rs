//! `one-armed-if`: a two-argument if with no else branch ((if test then) is (when test then)).
//!
//! The analysis lives in [`crate::one_armed_if::domain`], which also backs the
//! standalone `inspect one-armed-if` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::one_armed_if::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "one-armed-if",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-argument if with no else branch ((if test then) is (when test then))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::single(
                    item.head_span,
                    "when".to_owned(),
                    "Rewrite the else-less if as when".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "if has no else branch; (if test then) is (when test then)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
