//! `identity-arithmetic`: an arithmetic form with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1)).
//!

use paredit_core_lint_engine::LintResult;

use crate::identity_arithmetic::domain::examine_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "identity-arithmetic",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an arithmetic form with a redundant identity operand ((+ x 0), (* x 1), (- x 0), (/ x 1))",
    Fixability::ReportOnly,
);

/// The four heads `examine_form` accepts.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("+"),
    NormalizedHead::new("*"),
    NormalizedHead::new("-"),
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, &mut arithmetic_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} has a redundant identity operand {} (it does nothing)",
                    item.operator, item.identity
                ),
            );
        }
        Ok(())
    }
}
