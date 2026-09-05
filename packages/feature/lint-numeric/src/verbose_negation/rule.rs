//! `verbose-negation`: negation written the long way ((- 0 x) and (* x -1) are (- x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::support::is_hard_quoted_at;
use crate::verbose_negation::domain::examine_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "verbose-negation",
    RuleCategory::Suspicious,
    Severity::Warning,
    "negation written the long way ((- 0 x) and (* x -1) are (- x))",
    Fixability::Fixable,
);

/// `-` for `(- 0 x)`, `*` for `(* x -1)`/`(* -1 x)`; mirrors `examine_form`'s
/// own `matches!(head, "-" | "*")` re-check.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("-"), NormalizedHead::new("*")];

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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, &mut arithmetic_form_count, &mut items);
        for item in items {
            // This rule is `Fixable`, so a finding inside hard-quoted data is
            // not merely noise: applying the fix rewrites a *data literal*.
            // **Preventive**: both of this rule's 2 hard-quoted findings over
            // the 28 827 parsed Common Lisp files were `(deftransform core:negate
            // (((n fixnum))) '(- 0 n))` in one clasp file, where the quoted list
            // is the transform's expansion and so really is code. The guard
            // costs those 2 and prevented none there. Two findings in one file
            // do not establish that hard-quoted `(- 0 x)` is systematically a
            // template — `sign-comparison`'s 64 and `nil-comparison`'s 57 show
            // the opposite population — so the silent-corruption risk decides.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Rewrite as unary `(- X)`, copying X's source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    format!("(- {})", context_slice(item.operand_span)),
                    "Use unary (- x) for negation".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "negation written the long way; use (- x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
