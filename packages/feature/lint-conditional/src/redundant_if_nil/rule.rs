//! `redundant-if-nil`: a three-argument if whose else branch is a redundant literal nil ((if c x nil) is (if c x)).
//!
//! The analysis lives in [`crate::domain::redundant_if_nil_report`], which also backs the
//! standalone `inspect redundant-if-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_if_nil_report::examine_if;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-if-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a three-argument if whose else branch is a redundant literal nil ((if c x nil) is (if c x))",
    Fixability::Fixable,
);

/// `examine_if` only ever matches an `if` head.
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
            let fix = {
                // Delete the ` nil` else (from the then branch's end to the nil's).

                RuleFix::multi(
                    "Drop the redundant nil else branch".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "if else branch is a redundant nil; (if c x nil) is (if c x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
