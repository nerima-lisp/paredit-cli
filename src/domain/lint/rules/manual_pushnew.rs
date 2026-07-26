//! `manual-pushnew`: a setf/setq that manually adjoins onto a variable ((setf x (adjoin e x)) is (pushnew e x)).
//!
//! The analysis lives in [`crate::domain::manual_pushnew_report`], which also backs the
//! standalone `inspect manual-pushnew` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::manual_pushnew_report::examine_assignment;
use crate::domain::sexpr::ExpressionView;

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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut assignment_form_count = 0;
        let mut items = Vec::new();
        examine_assignment(view, context.path(), &mut assignment_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
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
