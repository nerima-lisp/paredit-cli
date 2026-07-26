//! `nested-progn`: a multi-form progn nested directly inside another progn (its forms splice in).
//!
//! The analysis lives in [`crate::domain::nested_progn_report`], which also backs the
//! standalone `inspect nested-progn` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nested_progn_report::examine_progn;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multi-form progn nested directly inside another progn (its forms splice in)",
    Fixability::Fixable,
);

/// `examine_progn` only inspects a `progn` head's own children.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("progn")];

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
        let mut progn_form_count = 0;
        let mut items = Vec::new();
        examine_progn(view, context.path(), &mut progn_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Splice the inner progn's body (exact source) in place of the
                // whole `(progn …)` wrapper.

                RuleFix::single(
                    item.span,
                    context_slice(item.body_span),
                    "Splice the nested progn into the enclosing progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "progn with {} forms is nested directly in another progn; splice its forms in",
                    item.body_form_count
                ),
                fix,
            );
        }
        Ok(())
    }
}
