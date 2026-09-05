//! `explicit-step-delta`: an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::explicit_step_delta::domain::examine_step;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "explicit-step-delta",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an incf/decf with an explicit delta of 1, the default ((incf x 1) is (incf x))",
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
            // **Preventive**: over 28 827 parsed Common Lisp files all 4 of
            // this rule's hard-quoted findings were `(eval '(defun … (incf x
            // 1) …))` in one antique ACL2 file, where the quoted text is handed
            // straight back to the evaluator and so really is code. The guard
            // costs those 4 and prevented none there; it is here because a
            // `'(incf x 1)` held as data is the shape `nil-comparison` and
            // `sign-comparison` were measured making 121 times over, and this
            // rule would corrupt it just as silently.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Drop the redundant delta: (incf place 1) -> (incf place).

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    format!(
                        "({} {})",
                        context_slice(item.head_span),
                        context_slice(item.place_span)
                    ),
                    "Drop the explicit default delta of 1".to_owned(),
                )
            };
            let operator = item.operator;

            sink.report_fixed(
                span,
                format!("{operator} delta of 1 is the default; ({operator} x 1) is ({operator} x)"),
                fix,
            );
        }
        Ok(())
    }
}
