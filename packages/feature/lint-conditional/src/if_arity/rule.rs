//! `if-arity`: an if with the wrong number of arguments (Common Lisp if takes 2 or 3).
//!
//! The analysis lives in [`crate::if_arity::domain`], which also backs the
//! standalone `inspect if-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::if_arity::domain::examine_if;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "if-arity",
    RuleCategory::Arity,
    Severity::Error,
    "an if with the wrong number of arguments (Common Lisp if takes 2 or 3)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("if")];

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
        let mut if_form_count = 0;
        let mut items = Vec::new();
        examine_if(view, context.path(), &mut if_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!("if takes 2 or 3 arguments but has {}", item.argument_count),
            );
        }
        Ok(())
    }
}
