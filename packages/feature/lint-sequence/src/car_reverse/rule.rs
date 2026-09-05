//! `car-reverse`: a car of a reverse, a wasteful full copy ((car (reverse x)) is (car (last x))).
//!

use paredit_core_lint_engine::LintResult;

use crate::car_reverse::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "car-reverse",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a car of a reverse, a wasteful full copy ((car (reverse x)) is (car (last x)))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("car"), NormalizedHead::new("first")];

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
        let mut accessor_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut accessor_form_count, &mut items);
        for item in items {
            let span = item.span;
            // Rewriting hard-quoted data edits a user's data literal rather than
            // code, and no round-trip property catches it. Read on the `hard`
            // counter alone: a `` `(…) `` template's contents really are emitted as
            // code. See `support::is_hard_quoted_at`.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (car (reverse x)) is (car (last x)), keeping the outer accessor.
                let text = format!(
                    "({} (last {}))",
                    context_slice(item.accessor_span),
                    context_slice(item.list_span)
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
                    "Rewrite (car (reverse x)) as (car (last x))".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "car of a reverse copies the whole list to read one element; use (car (last x))"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
