//! `format-newline`: a (format t "~%"), which is just (terpri) (write a newline to standard output).
//!
//! The analysis lives in [`crate::format_newline::domain`], which also backs the
//! standalone `inspect format-newline` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::format_newline::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "format-newline",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (format t \"~%\"), which is just (terpri) (write a newline to standard output)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("format")];

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
        let mut format_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut format_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (format t "~%") is (terpri).

                RuleFix::single(
                    item.span,
                    "(terpri)".to_owned(),
                    "Rewrite (format t \"~%\") as (terpri)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "(format t \"~%\") just writes a newline; use (terpri)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
