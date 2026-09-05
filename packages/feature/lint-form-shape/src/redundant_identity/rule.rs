//! `redundant-identity`: an identity call, which returns its argument unchanged ((identity x) is just x).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_identity::domain::examine_identity;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-identity",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an identity call, which returns its argument unchanged ((identity x) is just x)",
    Fixability::Fixable,
);

/// `examine_identity` only ever matches an `identity` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("identity")];

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
        let mut identity_form_count = 0;
        let mut items = Vec::new();
        examine_identity(view, &mut identity_form_count, &mut items);
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
                // Replace `(identity X)` with X's exact source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    context_slice(item.inner_span),
                    "Drop the redundant identity call".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "identity returns its argument unchanged; (identity x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
