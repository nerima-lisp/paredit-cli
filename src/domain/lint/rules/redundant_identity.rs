//! `redundant-identity`: an identity call, which returns its argument unchanged ((identity x) is just x).
//!
//! The analysis lives in [`crate::domain::redundant_identity_report`], which also backs the
//! standalone `inspect redundant-identity` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_identity_report::examine_identity;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-identity",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an identity call, which returns its argument unchanged ((identity x) is just x)",
    Fixability::Fixable,
);

/// `examine_identity` only ever matches an `identity` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("identity")];

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
        let mut identity_form_count = 0;
        let mut items = Vec::new();
        examine_identity(view, context.path(), &mut identity_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Replace `(identity X)` with X's exact source.

                RuleFix::single(
                    item.span,
                    context_slice(item.inner_span),
                    "Drop the redundant identity call".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "identity returns its argument unchanged; (identity x) is x".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
