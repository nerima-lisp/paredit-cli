//! `unwind-protect-no-cleanup`: an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x).
//!

use paredit_core_lint_engine::LintResult;

use crate::support::is_hard_quoted_at;
use crate::unwind_protect_no_cleanup::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "unwind-protect-no-cleanup",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("unwind-protect")];

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
        let mut unwind_protect_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut unwind_protect_form_count, &mut items);
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
                // (unwind-protect x) is x.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.form_span),
                    "Drop the cleanupless unwind-protect".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "an unwind-protect with no cleanup is just its body; (unwind-protect x) is x"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
