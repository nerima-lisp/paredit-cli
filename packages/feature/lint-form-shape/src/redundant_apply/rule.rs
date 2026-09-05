//! `redundant-apply`: an apply of a sharp-quoted symbol to a literal list ((apply #'foo (list a b)) is (foo a b)).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_apply::domain::examine_apply;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-apply",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an apply of a sharp-quoted symbol to a literal list ((apply #'foo (list a b)) is (foo a b))",
    Fixability::Fixable,
);

/// `examine_apply` only ever matches an `apply` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("apply")];

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
        let mut apply_form_count = 0;
        let mut items = Vec::new();
        examine_apply(view, &mut apply_form_count, &mut items);
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
                // Reconstruct the direct call `(callee args…)`, copying the list's
                // element source; an empty `(list)` yields a zero-argument call.
                let text = match item.args_span {
                    Some(args) => format!("({} {})", item.callee, context_slice(args)),
                    None => format!("({})", item.callee),
                };

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    format!(
                        "Rewrite (apply #'{} (list …)) as a direct call",
                        item.callee
                    ),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "apply of #'{} to a literal list is a direct call; use ({} …)",
                    item.callee, item.callee
                ),
                fix,
            );
        }
        Ok(())
    }
}
