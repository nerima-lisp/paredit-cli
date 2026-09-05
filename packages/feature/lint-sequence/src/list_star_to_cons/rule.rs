//! `list-star-to-cons`: a two-argument list*, which is just a cons ((list* a b) is (cons a b)).
//!

use paredit_core_lint_engine::LintResult;

use crate::list_star_to_cons::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "list-star-to-cons",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-argument list*, which is just a cons ((list* a b) is (cons a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("list*")];

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
        let mut list_star_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut list_star_form_count, &mut items);
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
                // (list* a b) is (cons a b).
                let text = format!(
                    "(cons {} {})",
                    context_slice(item.car_span),
                    context_slice(item.cdr_span)
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
                    "Rewrite (list* a b) as (cons a b)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a two-argument list* is just a cons; (list* a b) is (cons a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
