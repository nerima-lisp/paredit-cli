//! `cond-t-clause`: a cond with one t clause that has a body ((cond (t a b)) is (progn a b)).
//!
//! The analysis lives in [`crate::cond_t_clause::domain`], which also backs the
//! standalone `inspect cond-t-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::cond_t_clause::domain::examine_cond;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "cond-t-clause",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a cond with one t clause that has a body ((cond (t a b)) is (progn a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("cond")];

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
        let mut cond_form_count = 0;
        let mut items = Vec::new();
        examine_cond(view, context.path(), &mut cond_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (cond (t body…)) -> (progn body…): splice the body verbatim.

                RuleFix::single(
                    item.span,
                    format!("(progn {})", context_slice(item.body_span).trim()),
                    "Rewrite the single t-clause cond as progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "single t-clause cond is just progn; (cond (t a b)) is (progn a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
