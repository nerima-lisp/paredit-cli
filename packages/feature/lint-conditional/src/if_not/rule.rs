//! `if-not`: a three-argument if with then=nil and else=t ((if test nil t) is (not test)).
//!
//! The analysis lives in [`crate::if_not::domain`], which also backs the
//! standalone `inspect if-not` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::if_not::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "if-not",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a three-argument if with then=nil and else=t ((if test nil t) is (not test))",
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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, context.source(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (if test nil t) is (not test): keep the test verbatim.
                let text = format!("(not {})", context_slice(item.test_span));

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (if test nil t) as (not test)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "if with then=nil and else=t is a negation; (if test nil t) is (not test)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
