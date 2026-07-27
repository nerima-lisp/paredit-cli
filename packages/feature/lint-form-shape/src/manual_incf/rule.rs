//! `manual-incf`: a setf/setq that manually increments a variable ((setf x (1+ x)) is (incf x)).
//!
//! The analysis lives in [`crate::manual_incf::domain`], which also backs the
//! standalone `inspect manual-incf` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::manual_incf::domain::examine_assignment;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "manual-incf",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a setf/setq that manually increments a variable ((setf x (1+ x)) is (incf x))",
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
        examine_assignment(view, context.path(), &mut assignment_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Reconstruct `(incf V)` / `(incf V D)` / `(decf …)` from exact
                // source slices of the variable and (when present) the delta.
                let place = context_slice(item.place_span);
                let text = match item.delta_span {
                    Some(delta) => format!(
                        "({} {} {})",
                        item.suggested_head,
                        place,
                        context_slice(delta)
                    ),
                    None => format!("({} {})", item.suggested_head, place),
                };

                RuleFix::single(
                    item.span,
                    text,
                    format!("Rewrite the setf as {}", item.suggested_head),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "setf manually adjusts a variable; use {}",
                    item.suggested_head
                ),
                fix,
            );
        }
        Ok(())
    }
}
