//! `equality-arity`: an eq/eql/equal/equalp call without exactly two arguments.
//!
//! The analysis lives in [`crate::equality_arity::domain`], which also backs the
//! standalone `inspect equality-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::equality_arity::domain::examine_call;
use crate::support::is_eql_type_specifier_at;
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
            // `(eql object)` with exactly one argument is a CLHS 4.2.3 compound
            // *type specifier*, and in a type position one argument is not a
            // defect but the only legal spelling: `(defmethod g ((x (eql 7))) …)`
            // and `(typecase x ((eql 5) …))` are not one-argument calls.
            //
            // Narrowed to `eql`-at-one-argument on purpose, because that is
            // exactly what CLHS makes a specifier. `eq`, `equal` and `equalp`
            // name no type at all and `(eql a b c)` names none either, so no
            // other misarity shape can be silenced by this. The ancestor walk
            // runs only once a finding exists, so ordinary code never reaches
            // `root_view()`.
            if item.argument_count == 1
                && item.operator.eq_ignore_ascii_case("eql")
                && is_eql_type_specifier_at(context.tree(), span)
            {
                continue;
            }

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
