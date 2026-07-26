//! `gethash-default`: a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h)).
//!
//! The analysis lives in [`crate::domain::gethash_default_report`], which also backs the
//! standalone `inspect gethash-default` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::gethash_default_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "gethash-default",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a gethash with an explicit nil default, the default ((gethash k h nil) is (gethash k h))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("gethash")];

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
        let mut gethash_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut gethash_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Delete the redundant ` nil` default.

                RuleFix::multi(
                    "Drop the redundant nil default".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "the gethash default is nil; (gethash k h nil) is (gethash k h)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
