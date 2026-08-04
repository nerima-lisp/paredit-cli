//! `nil-comparison`: an eq/eql/equal/equalp comparison against nil ((eq x nil) is just (null x)).
//!
//! The analysis lives in [`crate::nil_comparison::domain`], which also backs the
//! standalone `inspect nil-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nil_comparison::domain::examine_comparison;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nil-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an eq/eql/equal/equalp comparison against nil ((eq x nil) is just (null x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("eq"),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("equalp"),
];

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
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, &mut comparison_form_count, &mut items);
        for item in items {
            // This rule is `Fixable`, so a finding inside hard-quoted data is
            // not merely noise: applying the fix rewrites a *data literal*.
            // Measured over 28 827 parsed Common Lisp files, 57 of this rule's
            // 5 877 fixes sat under a `'`, every one of them an ACL2 theorem
            // body inside a `'(progn …)` template or a `(defconst *equal-tests*
            // '(("Equal0" (equal t nil)) …))` expectation table. Rewriting
            // `(equal x nil)` to `(null x)` there edits the user's data.
            // Asked only once a finding exists, so a file with no findings
            // never pays for it. A quasiquoted `` `(eq ,x nil) `` is a template
            // that becomes real code, so `hard` alone is read, never `is_data`.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Rewrite the whole form as `(null X)`, copying X's exact source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    format!("(null {})", context_slice(item.operand_span)),
                    format!("Rewrite ({} X nil) as (null X)", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!("{} against nil is a null test; use (null X)", item.operator),
                fix,
            );
        }
        Ok(())
    }
}
