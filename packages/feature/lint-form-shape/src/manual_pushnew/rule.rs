//! `manual-pushnew`: a setf/setq that manually adjoins onto a variable ((setf x (adjoin e x)) is (pushnew e x)).
//!
//! The analysis lives in [`crate::manual_pushnew::domain`], which also backs the
//! standalone `inspect manual-pushnew` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::manual_pushnew::domain::examine_assignment;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "manual-pushnew",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a setf/setq that manually adjoins onto a variable ((setf x (adjoin e x)) is (pushnew e x))",
    Fixability::Fixable,
);

/// The two assignment heads `examine_assignment` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("setf"), NormalizedHead::new("setq")];

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
        let mut assignment_form_count = 0;
        let mut items = Vec::new();
        examine_assignment(
            view,
            context.source(),
            &mut assignment_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // Reconstruct `(pushnew E P KW…)` by reusing adjoin's operand list.
                let text = format!("(pushnew {})", context_slice(item.args_span));

                RuleFix::single(item.span, text, "Rewrite the setf as pushnew".to_owned())
            };

            sink.report_fixed(
                span,
                "setf adjoins onto a variable; use pushnew".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
