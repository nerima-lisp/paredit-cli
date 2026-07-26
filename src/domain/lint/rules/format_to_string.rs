//! `format-to-string`: a (format nil "~A"/"~S" x), which is (princ-to-string x)/(prin1-to-string x).
//!
//! The analysis lives in [`crate::domain::format_to_string_report`], which also backs the
//! standalone `inspect format-to-string` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::format_to_string_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "format-to-string",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (format nil \"~A\"/\"~S\" x), which is (princ-to-string x)/(prin1-to-string x)",
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
        let context_slice = |span| context.slice(span).to_owned();
        let mut format_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut format_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (format nil "~A"/"~S" x) is (princ-to-string x)/(prin1-to-string x).
                let text = format!(
                    "({} {})",
                    item.replacement,
                    context_slice(item.argument_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    format!("Rewrite (format nil … x) as ({} x)", item.replacement),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "format to a string is just {}; use ({} x)",
                    item.replacement, item.replacement
                ),
                fix,
            );
        }
        Ok(())
    }
}
