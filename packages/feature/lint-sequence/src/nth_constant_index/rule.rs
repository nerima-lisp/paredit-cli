//! `nth-constant-index`: an nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x)).
//!
//! The analysis lives in [`crate::nth_constant_index::domain`], which also backs the
//! standalone `inspect nth-constant-index` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nth_constant_index::domain::examine_nth;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nth-constant-index",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("nth")];

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
        let mut nth_form_count = 0;
        let mut items = Vec::new();
        examine_nth(view, &mut nth_form_count, &mut items);
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
                // Rewrite `(nth N x)` as `(ordinal x)`, copying the list source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    format!("({} {})", item.ordinal, context_slice(item.list_span)),
                    format!(
                        "Use ({} …) instead of nth with a constant index",
                        item.ordinal
                    ),
                )
            };

            sink.report_fixed(
                span,
                format!("nth with a constant index; use ({} …)", item.ordinal),
                fix,
            );
        }
        Ok(())
    }
}
