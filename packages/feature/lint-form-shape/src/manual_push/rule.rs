//! `manual-push`: a setf/setq that manually conses onto a variable ((setf x (cons e x)) is (push e x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::manual_push::domain::examine_assignment;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "manual-push",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a setf/setq that manually conses onto a variable ((setf x (cons e x)) is (push e x))",
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
        examine_assignment(view, &mut assignment_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // Reconstruct `(push E P)` from exact source slices of the pushed
                // element and the place variable.
                let text = format!(
                    "(push {} {})",
                    context_slice(item.element_span),
                    context_slice(item.place_span)
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
                    "Rewrite the setf as push".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "setf conses onto a variable; use push".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
