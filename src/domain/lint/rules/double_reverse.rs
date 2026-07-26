//! `double-reverse`: a (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x)).
//!
//! The analysis lives in [`crate::domain::double_reverse_report`], which also backs the
//! standalone `inspect double-reverse` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::double_reverse_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "double-reverse",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("reverse")];

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
        let mut reverse_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut reverse_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (reverse (reverse x)) is (copy-seq x): keep the inner argument.
                let text = format!("(copy-seq {})", context_slice(item.inner_span));

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (reverse (reverse x)) as (copy-seq x)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "(reverse (reverse x)) is a wasteful copy; use (copy-seq x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
