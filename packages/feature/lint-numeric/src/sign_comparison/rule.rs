//! `sign-comparison`: a =/</> comparison against 0 with a dedicated predicate ((= x 0) is (zerop x)).
//!
//! The analysis lives in [`crate::sign_comparison::domain`], which also backs the
//! standalone `inspect sign-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::sign_comparison::domain::examine_comparison;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "sign-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a =/</> comparison against 0 with a dedicated predicate ((= x 0) is (zerop x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("="),
    NormalizedHead::new(">"),
    NormalizedHead::new("<"),
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
        let context_slice = |span| context.slice(span).to_owned();
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, context.path(), &mut comparison_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite the whole form as `(predicate X)`, copying X's source.

                RuleFix::single(
                    item.span,
                    format!("({} {})", item.predicate, context_slice(item.operand_span)),
                    format!("Use ({} X) instead of comparing against 0", item.predicate),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "comparison against 0 has a dedicated predicate; use {}",
                    item.predicate
                ),
                fix,
            );
        }
        Ok(())
    }
}
