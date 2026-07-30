//! `redundant-apply`: an apply of a sharp-quoted symbol to a literal list ((apply #'foo (list a b)) is (foo a b)).
//!
//! The analysis lives in [`crate::redundant_apply::domain`], which also backs the
//! standalone `inspect redundant-apply` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_apply::domain::examine_apply;
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
            let fix = {
                // Reconstruct the direct call `(callee args…)`, copying the list's
                // element source; an empty `(list)` yields a zero-argument call.
                let text = match item.args_span {
                    Some(args) => format!("({} {})", item.callee, context_slice(args)),
                    None => format!("({})", item.callee),
                };

                RuleFix::single(
                    item.span,
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
