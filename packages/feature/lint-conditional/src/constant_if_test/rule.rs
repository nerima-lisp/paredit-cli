//! `constant-if-test`: an if whose test is the literal t or nil ((if t a b) is a; (if nil a b) is b).
//!
//! The analysis lives in [`crate::constant_if_test::domain`], which also backs the
//! standalone `inspect constant-if-test` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::constant_if_test::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::NormalizedHead;
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::value::evaluate_constant;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "constant-if-test",
    RuleCategory::DeadCode,
    Severity::Warning,
    "an if whose test is the literal t or nil ((if t a b) is a; (if nil a b) is b)",
    Fixability::Fixable,
);

/// The only head `examine_if` accepts.
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
        // A test the value layer settles is as decided as a literal `t`: the
        // branch not taken is dead either way. `Unknown` leaves the form
        // alone, which is what keeps a genuine run-time condition untouched.
        let constant_test = |test: &ExpressionView| {
            evaluate_constant(
                context.dialect(),
                test,
                context.binding_table(),
                context.value_table(),
            )
            .truthiness(context.dialect())
        };

        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(
            view,
            context.source(),
            &constant_test,
            &mut if_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // Replace the whole form with the live branch (or `nil` for a false
                // one-armed if), dropping the dead branch.
                let text = match item.result_span {
                    Some(span) => context_slice(span),
                    None => "nil".to_owned(),
                };

                RuleFix::single(
                    item.span,
                    text,
                    format!("Drop the dead branch of the constant {} test", item.test),
                )
            };

            sink.report_fixed(
                span,
                format!("if test is the constant {}; one branch is dead", item.test),
                fix,
            );
        }
        Ok(())
    }
}
