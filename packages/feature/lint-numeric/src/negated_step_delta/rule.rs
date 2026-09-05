//! `negated-step-delta`: an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::negated_step_delta::domain::examine_step;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-step-delta",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an incf/decf with a negative literal delta, which flips the operator ((incf x -1) is (decf x))",
    Fixability::Fixable,
);

/// The two heads `examine_step` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("incf"), NormalizedHead::new("decf")];

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
        let mut step_form_count = 0;
        let mut items = Vec::new();
        examine_step(view, &mut step_form_count, &mut items);
        for item in items {
            // This rule is `Fixable`, so a finding inside hard-quoted data is
            // not merely noise: applying the fix rewrites a *data literal*.
            // **Preventive**: this rule made no findings at all over the 28 827
            // parsed Common Lisp files the guard was measured on, so it has no
            // observed misfires and the guard costs nothing observed either.
            // It is here because `'(incf x -1)` held as data is the same shape
            // the sibling rules were measured corrupting, and a rule that
            // rewrites the operator *and* the literal has more to corrupt than
            // most.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Flip the operator and drop the sign: (incf x -5) -> (decf x 5).
                let magnitude = context_slice(item.delta_span);
                let magnitude = magnitude.strip_prefix('-').unwrap_or(&magnitude);
                let text = format!(
                    "({} {} {})",
                    item.opposite,
                    context_slice(item.place_span),
                    magnitude
                );

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    format!("Use {} with a positive delta", item.opposite),
                )
            };
            let opposite = item.opposite;

            sink.report_fixed(
                span,
                format!("negative delta flips the operator; use {opposite} with a positive delta"),
                fix,
            );
        }
        Ok(())
    }
}
