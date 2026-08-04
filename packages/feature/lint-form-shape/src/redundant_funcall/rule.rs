//! `redundant-funcall`: a funcall of a sharp-quoted symbol ((funcall #'foo a b) is just (foo a b)).
//!
//! The analysis lives in [`crate::redundant_funcall::domain`], which also backs the
//! standalone `inspect redundant-funcall` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_funcall::domain::examine_funcall;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-funcall",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a funcall of a sharp-quoted symbol ((funcall #'foo a b) is just (foo a b))",
    Fixability::Fixable,
);

/// `examine_funcall` only ever matches a `funcall` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("funcall")];

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
        let mut funcall_form_count = 0;
        let mut items = Vec::new();
        examine_funcall(view, &mut funcall_form_count, &mut items);
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
                // Delete `funcall ` and the `#'` prefix in one cut, from the funcall
                // head up to the callee symbol, leaving `(foo …)` byte-identical.

                RuleFix::multi(
                    format!("Rewrite (funcall #'{} …) as a direct call", item.callee),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "funcall of #'{} is a direct call; (funcall #'{} …) is ({} …)",
                    item.callee, item.callee, item.callee
                ),
                fix,
            );
        }
        Ok(())
    }
}
