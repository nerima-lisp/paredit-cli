//! `redundant-precision-coercion`: a float conversion discarded by the
//! truncation wrapped immediately around it.
//!
//! The analysis lives in [`crate::redundant_precision_coercion::domain`], which
//! also backs the standalone `inspect redundant-precision-coercion` command;
//! this module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_precision_coercion::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-precision-coercion",
    RuleCategory::NumericPrecision,
    Severity::Warning,
    "a float coercion immediately discarded by a truncate/floor/ceiling/round around it",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "coerce rounds to the nearest representable float, and truncate is discontinuous at every \
         integer, so the conversion can be amplified into a full unit of error before it is \
         thrown away. SBCL evaluates (truncate 123456789123456789) to itself but (truncate \
         (float 123456789123456789)) to 123456790519087104. A rational just below an integer can \
         also round up across the boundary: (truncate 99999999999999999999/100000000000000000000) \
         is 0, and coercing first makes it 1.",
    )
    .with_example("(truncate (coerce x 'double-float))", "(truncate x)")
    .with_caveat(
        "Deliberately not fixable. The two forms are different functions on exactly the inputs \
         above, so removing the coercion — or adding one — is a silent change of result, not a \
         simplification. Only the author knows which was meant.",
    ),
);

/// The four heads `examine` accepts.
///
/// The `f`-prefixed family (`ffloor`, `fround`, …) is absent on purpose: those
/// return floats, so a float argument is not discarded.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("truncate"),
    NormalizedHead::new("floor"),
    NormalizedHead::new("ceiling"),
    NormalizedHead::new("round"),
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
        let mut truncation_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut truncation_form_count, &mut items);
        for item in items {
            // Asked only once a finding exists: the dispatcher hands a rule
            // quoted nodes like any other, and `'(truncate (float x))` is a
            // literal list.
            if is_unevaluated_at(context.tree(), item.span) {
                continue;
            }
            let span = item.span;
            sink.report(span, item.message());
        }
        Ok(())
    }
}
