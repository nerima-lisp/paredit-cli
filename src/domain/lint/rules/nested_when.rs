//! `nested-when`: a when whose only body is a when, mergeable by and ((when a (when b c)) is (when (and a b) c)).
//!
//! The analysis lives in [`crate::domain::nested_when_report`], which also backs the
//! standalone `inspect nested-when` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nested_when_report::examine_when;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-when",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a when whose only body is a when, mergeable by and ((when a (when b c)) is (when (and a b) c))",
    Fixability::Fixable,
);

/// `examine_when` only ever matches a `when` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("when")];

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
        let mut when_form_count = 0;
        let mut items = Vec::new();
        examine_when(view, context.path(), &mut when_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Merge the two tests: (when a (when b body)) -> (when (and a b) body).
                let and = format!(
                    "(and {} {})",
                    context_slice(item.outer_test_span),
                    context_slice(item.inner_test_span)
                );
                let text = match item.inner_body_span {
                    Some(body) => format!("(when {} {})", and, context_slice(body)),
                    None => format!("(when {})", and),
                };

                RuleFix::single(
                    item.span,
                    text,
                    "Merge the nested when tests with and".to_owned(),
                )
            };

            sink.report_fixed(span, "when whose only body is a when merges by and; (when a (when b c)) is (when (and a b) c)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
