//! `epsilon-less-float-loop-bound`: a `do`/`do*` whose float accumulator is
//! tested for exact equality against a bound that is not a float literal.
//!
//! The analysis lives in [`crate::epsilon_less_float_loop_bound::domain`], which
//! also backs the standalone `inspect epsilon-less-float-loop-bound` command;
//! this module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::epsilon_less_float_loop_bound::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "epsilon-less-float-loop-bound",
    RuleCategory::NumericPrecision,
    Severity::Warning,
    "a do loop stepping an inexact float that terminates on = or eql rather than an ordered \
     comparison",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Repeated addition of a float that binary floating point cannot hold exactly accumulates \
         error, so the running value steps past the bound rather than landing on it. Adding 0.1 \
         ten times gives 1.0000001 in single-float and 0.9999999999999999 in double-float — it \
         overshoots in one format and undershoots in the other — so an = or eql end test may \
         never hold and the loop runs forever. An ordered comparison stops on the first value \
         past the bound and is correct for any step.",
    )
    .with_example(
        "(do ((x 0.0 (+ x 0.1))) ((= x 1)) (body))",
        "(do ((x 0.0 (+ x 0.1))) ((>= x 1)) (body))",
    )
    .with_caveat(
        "A step that is exactly representable — 0.5, 0.25, 0.125 — accumulates without drift and \
         is left alone. A test holding a written-out float literal is `float-equality`'s finding, \
         not this one's, so the two rules never both fire on one form.",
    ),
);

/// The two heads `examine` accepts.
///
/// `dotimes` is deliberately absent: CLHS gives it no step form at all and
/// requires its count form to produce an integer, so a drifting float
/// accumulator cannot be written with it. `loop`'s clause grammar is
/// `inspect loop`'s subject and guessing at it here would be a false-positive
/// source.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("do"), NormalizedHead::new("do*")];

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
        let mut do_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut do_form_count, &mut items);
        for item in items {
            // Asked only once a finding exists: the dispatcher hands a rule
            // quoted nodes like any other, and `'(do ((x 0.0 (+ x 0.1))) ((= x
            // 1)))` is a literal list.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            sink.report(span, item.message());
        }
        Ok(())
    }
}
