//! `mixed-float-precision-arithmetic`: a single-float literal beside a
//! double-float literal in one arithmetic form.
//!

use paredit_core_lint_engine::LintResult;

use crate::mixed_float_precision_arithmetic::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "mixed-float-precision-arithmetic",
    RuleCategory::NumericPrecision,
    Severity::Warning,
    "a single-float literal beside a double-float literal in one arithmetic form, capping the \
     result at single precision",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "CLHS 12.1.4.4 makes the result of a mixed form a float of the largest format present, so \
         a form holding a double-float literal produces a double-float. A single-float literal in \
         the same form is widened first, and widening preserves the single-float rounding error \
         rather than removing it: SBCL evaluates (* 3.14 1.0d0) to 3.140000104904175d0. The \
         result carries a double-float's type and a single-float's accuracy.",
    )
    .with_example("(* 3.14 1.0d0)", "(* 3.14d0 1.0d0)")
    .with_caveat(
        "Only reported when the single-float literal actually changes value on widening. \
         (* 1.5 1.0d0) is 1.5d0 exactly, so it is left alone — as is every mix of an exact \
         integer or ratio with a float, which is ordinary, fully-specified contagion and not a \
         defect.",
    ),
);

/// The four heads `examine` accepts.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("+"),
    NormalizedHead::new("-"),
    NormalizedHead::new("*"),
    NormalizedHead::new("/"),
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
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut arithmetic_form_count, &mut items);
        for item in items {
            // Asked only once a finding exists: `'(+ 3.14 1.0d0)` is a list of
            // four symbols, and the dispatcher hands a rule quoted nodes like
            // any other.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            // The report's own sentence, so a consumer reading the lint suite
            // and `inspect` sees one finding described one way.
            sink.report(span, item.message());
        }
        Ok(())
    }
}
