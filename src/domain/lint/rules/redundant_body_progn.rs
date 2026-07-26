//! `redundant-body-progn`: a multi-form progn used as a when/unless/let/defun/... body (its forms splice in).
//!
//! The analysis lives in [`crate::domain::redundant_body_progn_report`], which also backs the
//! standalone `inspect redundant-body-progn` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_body_progn_report::examine_form;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-body-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multi-form progn used as a when/unless/let/defun/... body (its forms splice in)",
    Fixability::Fixable,
);

/// Every implicit-progn head `body_start` recognizes (`redundant_body_progn_report`).
const HEADS: [NormalizedHead; 14] = [
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("dotimes"),
    NormalizedHead::new("block"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("macrolet"),
    NormalizedHead::new("catch"),
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
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
        let mut implicit_progn_form_count = 0;
        let mut items = Vec::new();
        examine_form(
            view,
            context.path(),
            &mut implicit_progn_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Splice the progn's body (exact source) in place of the wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.body_span),
                    format!("Splice the progn into the enclosing {}", item.parent),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "progn with {} forms is a {} body; splice its forms in",
                    item.body_form_count, item.parent
                ),
                fix,
            );
        }
        Ok(())
    }
}
