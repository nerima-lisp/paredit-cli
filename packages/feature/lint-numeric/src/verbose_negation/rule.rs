//! `verbose-negation`: negation written the long way ((- 0 x) and (* x -1) are (- x)).
//!
//! The analysis lives in [`crate::verbose_negation::domain`], which also backs the
//! standalone `inspect verbose-negation` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::verbose_negation::domain::examine_form;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "verbose-negation",
    RuleCategory::Suspicious,
    Severity::Warning,
    "negation written the long way ((- 0 x) and (* x -1) are (- x))",
    Fixability::Fixable,
);

/// `-` for `(- 0 x)`, `*` for `(* x -1)`/`(* -1 x)`; mirrors `examine_form`'s
/// own `matches!(head, "-" | "*")` re-check.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("-"), NormalizedHead::new("*")];

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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut arithmetic_form_count = 0;
        let mut items = Vec::new();
        examine_form(view, context.path(), &mut arithmetic_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Rewrite as unary `(- X)`, copying X's source.

                RuleFix::single(
                    item.span,
                    format!("(- {})", context_slice(item.operand_span)),
                    "Use unary (- x) for negation".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "negation written the long way; use (- x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
