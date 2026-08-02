//! `division-result-precision-loss`: an Emacs Lisp integer division whose
//! quotient truncates to zero.
//!
//! The analysis lives in [`crate::division_result_precision_loss::domain`],
//! which also backs the standalone `inspect division-result-precision-loss`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::division_result_precision_loss::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "division-result-precision-loss",
    RuleCategory::NumericPrecision,
    Severity::Warning,
    "an Emacs Lisp integer division whose quotient truncates to zero, discarding the value",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "The GNU Emacs Lisp Reference Manual says of `/`: \"If all the arguments are integers, \
         the result is an integer, obtained by rounding the quotient towards zero after each \
         division.\" So (/ 1 3) is 0, not one third. This is the opposite of Common Lisp, where \
         the same form is the exact ratio 1/3 and nothing is lost — which is why this rule is \
         Emacs Lisp only.",
    )
    .with_example("(/ 1 3)", "(/ 1.0 3)")
    .with_caveat(
        "Only a quotient that collapses a non-zero numerator to 0 is reported. (/ 100 3) is 33 \
         and is very plausibly deliberate integer division, so it is left alone. Single-argument \
         `/` is skipped too: its meaning changed in Emacs 25.1.",
    ),
);

/// The one head `examine` accepts.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("/")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Emacs Lisp only, and this is the whole point of the rule rather than an
    /// incidental restriction: in Common Lisp `(/ 1 3)` is the exact ratio
    /// `1/3`, so there is no precision to lose. The dispatcher drops the rule
    /// before the walk for every other dialect, which is also why it costs a
    /// Common Lisp file nothing at all.
    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut division_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut division_form_count, &mut items);
        for item in items {
            // Asked only once a finding exists: `'(/ 1 3)` is a three-element
            // literal list, and the dispatcher hands a rule quoted nodes like
            // any other.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            sink.report(span, item.message());
        }
        Ok(())
    }
}
