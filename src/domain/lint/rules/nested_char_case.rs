//! `nested-char-case`: nested char case ops where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c)).
//!
//! The analysis lives in [`crate::domain::nested_char_case_report`], which also backs the
//! standalone `inspect nested-char-case` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nested_char_case_report::examine;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-char-case",
    RuleCategory::Suspicious,
    Severity::Warning,
    "nested char case ops where the outer dominates ((char-upcase (char-downcase c)) is (char-upcase c))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("char-upcase"),
    NormalizedHead::new("char-downcase"),
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
    ) -> Result<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut char_case_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut char_case_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (OUTER (INNER c)) is (OUTER c), keeping the outer op.
                let text = format!(
                    "({} {})",
                    context_slice(item.outer_span),
                    context_slice(item.char_span)
                );

                RuleFix::single(
                    item.span,
                    text,
                    "Collapse the nested char case op".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "the outer char case op dominates; the inner one is dead work".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
