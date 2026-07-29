//! `eq-number-comparison`: an eq compared against a number literal (eq on numbers is unreliable).
//!
//! The analysis lives in [`crate::eq_number_comparison::domain`], which also backs the
//! standalone `inspect eq-number-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::eq_number_comparison::domain::{NumberEvidence, examine_comparison};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_semantics::semantics::NodeKey;
use paredit_core_semantics::semantics::typing::Ty;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "eq-number-comparison",
    RuleCategory::Suspicious,
    Severity::Error,
    "an eq compared against a number literal (eq on numbers is unreliable)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("eq")];

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
        // The type context turns "spelled as a number" into "is a number".
        // `Unknown` says nothing, which is what keeps this silent on an
        // argument the layer could not settle; `Bottom` is excluded because it
        // is a subtype of everything, so a binding whose declarations
        // contradict would otherwise answer "definitely a number" — that is
        // dead code, not an `eq` bug.
        let is_number = |argument: &ExpressionView| {
            NodeKey::of(argument).is_some_and(|key| {
                let ty = context.type_table().expression_type(key);
                ty.is_definitely(Ty::Number) && !ty.is_definitely(Ty::Bottom)
            })
        };

        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(
            view,
            context.source(),
            &is_number,
            &mut comparison_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::single(
                    item.head_span,
                    "eql".to_owned(),
                    "Compare with eql (eq is unreliable on numbers)".to_owned(),
                )
            };

            let message = match &item.evidence {
                NumberEvidence::Literal(literal) => {
                    format!("eq compares against number literal {literal}")
                }
                NumberEvidence::InferredType => {
                    "eq compares against an argument of inferred type number".to_owned()
                }
            };

            sink.report_fixed(span, message, fix);
        }
        Ok(())
    }
}
