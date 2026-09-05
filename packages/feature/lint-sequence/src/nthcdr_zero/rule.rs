//! `nthcdr-zero`: an nthcdr with a zero count, which returns the list unchanged ((nthcdr 0 x) is x).
//!

use paredit_core_lint_engine::LintResult;

use crate::nthcdr_zero::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nthcdr-zero",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an nthcdr with a zero count, which returns the list unchanged ((nthcdr 0 x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("nthcdr")];

/// Whether `list` — the source this rule would put in the whole form's place —
/// begins with a splicing unquote.
///
/// Alone among the five rules this package re-spanned, this one's replacement
/// text *is* an operand's own source rather than a freshly parenthesized
/// `(head …)`, so the operand's leading reader syntax ends up where the form
/// stood. Two independent reasons to decline, either sufficient:
///
/// - The operand count is expansion-dependent. `(nthcdr 0 ,@xs)` is the
///   two-operand `nthcdr` this rule reasons about only when `xs` happens to
///   expand to exactly one form, so the domain's `children.len() == 3` never
///   established the premise.
/// - There is no well-formed rewrite to emit. The fix replaces the form's
///   *contents*, so `` `(nthcdr 0 ,@xs) `` would become `` `,@xs `` — which
///   SBCL refuses to read, a splicing unquote having no list to splice into
///   there. Verified against SBCL's reader rather than assumed.
///
/// Keyed on the replacement's leading characters and not on the operand's
/// `ReaderPrefix` list, because the parser labels `,.` — which splices exactly
/// as `,@` does, and which `` ` `` rejects identically (also verified) — as
/// `ReaderPrefix::Unquote`. That mislabelling is a real separate defect,
/// recorded in PR #127 and worked around here rather than repaired; reading the
/// text is what keeps this guard from inheriting it.
///
/// The sibling rule `coerce-to-t` carries the same guard for the same reason.
/// The other four re-spanned rules build their replacement as `format!("(… )")`
/// and so are unreachable this way by construction: whatever the operands carry
/// ends up *inside* parentheses, where a splice is well-formed.
fn is_spliced_operand(list: &str) -> bool {
    list.starts_with(",@") || list.starts_with(",.")
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
        let mut nthcdr_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut nthcdr_form_count, &mut items);
        for item in items {
            let span = item.span;
            // Rewriting hard-quoted data edits a user's data literal rather than
            // code, and no round-trip property catches it. Read on the `hard`
            // counter alone: a `` `(…) `` template's contents really are emitted as
            // code. See `support::is_hard_quoted_at`.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            // (nthcdr 0 x) is x: the list operand's own source stands in for the
            // whole form.
            let list = context_slice(item.list_span);
            // A spliced operand leaves this rule with neither an established
            // premise nor a well-formed rewrite; see `is_spliced_operand`.
            if is_spliced_operand(&list) {
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
                    list,
                    "Drop the no-op (nthcdr 0 …)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "nthcdr with a zero count returns the list unchanged; (nthcdr 0 x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
