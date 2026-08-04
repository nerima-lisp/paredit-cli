//! `equality-arity`: an eq/eql/equal/equalp call without exactly two arguments.
//!
//! The analysis lives in [`crate::equality_arity::domain`], which also backs the
//! standalone `inspect equality-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::equality_arity::domain::examine_call;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "equality-arity",
    RuleCategory::Arity,
    Severity::Error,
    "an eq/eql/equal/equalp call without exactly two arguments",
    Fixability::ReportOnly,
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
        let mut call_count = 0;
        let mut items = Vec::new();
        examine_call(view, &mut call_count, &mut items);
        for item in items {
            // `examine_call` already declines a node carrying its *own* `'`, but
            // the arity of a form nested inside an enclosing quote is just as
            // meaningless: `'(cons (eql function) null)` is a type specifier
            // SBCL writes throughout its own sources, and `(eql function)` there
            // is a datum, not a one-argument call. Asked only once a finding
            // exists, so ordinary code never reaches `root_view()`. A
            // quasiquoted `` `(eql ,a ,b) `` is a template that becomes a real
            // call, and stays reported.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} takes exactly 2 arguments but has {}",
                    item.operator, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
