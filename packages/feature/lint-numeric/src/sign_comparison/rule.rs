//! `sign-comparison`: a =/</> comparison against 0 with a dedicated predicate ((= x 0) is (zerop x)).
//!
//! The analysis lives in [`crate::sign_comparison::domain`], which also backs the
//! standalone `inspect sign-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::sign_comparison::domain::examine_comparison;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "sign-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a =/</> comparison against 0 with a dedicated predicate ((= x 0) is (zerop x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("="),
    NormalizedHead::new(">"),
    NormalizedHead::new("<"),
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
            // Measured over 28 827 parsed Common Lisp files, 64 of this rule's
            // 36 761 fixes sat under a `'`, and they are ACL2 `:hints '(:cases
            // ((< 0 a) …))` proof terms and `(defconst *<-tests* '(("Less…" (<
            // 0 1)) …))` expectation tables — rewriting `(< 0 a)` to `(plusp
            // a)` there edits the user's data, not their code. Asked only once
            // a finding exists, so a file with no findings never pays for it.
            // A quasiquoted `` `(< ,x 0) `` is a template that becomes real
            // code, so `hard` alone is read and never `is_data`.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Rewrite the whole form as `(predicate X)`, copying X's source.

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    format!("({} {})", item.predicate, context_slice(item.operand_span)),
                    format!("Use ({} X) instead of comparing against 0", item.predicate),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "comparison against 0 has a dedicated predicate; use {}",
                    item.predicate
                ),
                fix,
            );
        }
        Ok(())
    }
}
