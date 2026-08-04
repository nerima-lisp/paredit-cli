//! `coerce-to-t`: a coerce to type t, which returns the object unchanged ((coerce x t) is x).
//!
//! The analysis lives in [`crate::coerce_to_t::domain`], which also backs the
//! standalone `inspect coerce-to-t` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::coerce_to_t::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "coerce-to-t",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a coerce to type t, which returns the object unchanged ((coerce x t) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("coerce")];

/// Whether `operand` — the source this rule would put in the whole form's
/// place — begins with a splicing unquote.
///
/// Alone among the rules here, this one's replacement text *is* an operand's
/// own source rather than a freshly parenthesized `(head …)`, so the operand's
/// leading reader syntax ends up where the form stood. Two independent reasons
/// to decline, either sufficient:
///
/// - The operand count is expansion-dependent. `(coerce ,@xs t)` is the
///   two-operand `coerce` this rule reasons about only when `xs` happens to
///   expand to exactly one form, so the domain's `children.len() == 3` never
///   established the premise. This is the same objection `redundant-progn`
///   raises against a `,@`-spliced sole body form.
/// - There is no well-formed rewrite to emit. The fix replaces the form's
///   *contents*, so `` `(coerce ,@xs t) `` would become `` `,@xs `` — which
///   SBCL refuses to read, a splicing unquote having no list to splice into
///   there. Verified against SBCL's reader rather than assumed.
///
/// Keyed on the replacement's leading characters and not on the operand's
/// `ReaderPrefix` list, because the parser labels `,.` — which splices exactly
/// as `,@` does, and which `` ` `` rejects identically (also verified) — as
/// `ReaderPrefix::Unquote`. That mislabelling is a real separate defect,
/// recorded in PR #127; reading the text is what keeps this guard from
/// inheriting it.
fn is_spliced_operand(operand: &str) -> bool {
    operand.starts_with(",@") || operand.starts_with(",.")
}

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
        let mut coerce_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut coerce_form_count, &mut items);
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
            // (coerce x t) is x.
            let object = context_slice(item.object_span);
            // A spliced operand leaves this rule with neither an established
            // premise nor a well-formed rewrite; see `is_spliced_operand`.
            if is_spliced_operand(&object) {
                continue;
            }
            let fix = {
                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    object,
                    "Drop the no-op (coerce x t)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "coerce to type t returns the object unchanged; (coerce x t) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
