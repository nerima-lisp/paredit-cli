//! `prog2-to-progn`: a two-form prog2, which is just progn ((prog2 a b) is (progn a b)).
//!
//! The analysis lives in [`crate::prog2_to_progn::domain`], which also backs the
//! standalone `inspect prog2-to-progn` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::prog2_to_progn::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "prog2-to-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-form prog2, which is just progn ((prog2 a b) is (progn a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("prog2")];

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
        let mut prog2_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut prog2_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite the operator token prog2 -> progn, keeping the two forms.

                RuleFix::single(
                    item.head_span,
                    "progn".to_owned(),
                    "Rewrite prog2 as progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a two-form prog2 is just progn; (prog2 a b) is (progn a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
