//! `one-step-arithmetic`: a +/- of a literal 1 with a shorthand ((+ x 1) is (1+ x); (- x 1) is (1- x)).
//!
//! The analysis lives in [`crate::one_step_arithmetic::domain`], which also backs the
//! standalone `inspect one-step-arithmetic` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::one_step_arithmetic::domain::examine_form;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "one-step-arithmetic",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a +/- of a literal 1 with a shorthand ((+ x 1) is (1+ x); (- x 1) is (1- x))",
    Fixability::Fixable,
);

/// The two heads `examine_form` accepts.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("+"), NormalizedHead::new("-")];

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
            // This rule is `Fixable`, so a finding inside quoted data is not
            // merely noise: applying the fix would rewrite `'(+ n 1)` — the
            // *expected* value of a test asserting what `1+` expands to, or a
            // program held as data for a macroexpander — into `'(1+ n)` and
            // invert the assertion. Asked only once a finding exists, so
            // ordinary code never reaches `root_view()`. A quasiquoted
            // `` `(+ ,x 1) `` is a template that becomes real arithmetic, and
            // stays reported.
            if is_hard_quoted_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            let fix = {
                // Rewrite as the unary shorthand: (+ x 1) -> (1+ x), (- x 1) -> (1- x).
                let text = format!("({} {})", item.shorthand, context_slice(item.operand_span));

                RuleFix::single(
                    item.span,
                    text,
                    format!("Use the {} shorthand", item.shorthand),
                )
            };
            let shorthand = item.shorthand;

            sink.report_fixed(
                span,
                format!("add/subtract of 1 has a shorthand; use {shorthand}"),
                fix,
            );
        }
        Ok(())
    }
}
