//! `append-list-to-cons`: a one-element append that is just a cons ((append (list x) rest) is (cons x rest)).
//!
//! The analysis lives in [`crate::append_list_to_cons::domain`], which also backs the
//! standalone `inspect append-list-to-cons` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::append_list_to_cons::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "append-list-to-cons",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a one-element append that is just a cons ((append (list x) rest) is (cons x rest))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("append")];

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
        let mut append_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut append_form_count, &mut items);
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
                // (append (list x) rest) is (cons x rest).
                let text = format!(
                    "(cons {} {})",
                    context_slice(item.element_span),
                    context_slice(item.rest_span)
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
                    "Rewrite (append (list x) rest) as (cons x rest)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a one-element append is just a cons; (append (list x) rest) is (cons x rest)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
