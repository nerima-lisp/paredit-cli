//! `getf-default-nil`: a getf call with an explicit nil default, the default ((getf p k nil) is (getf p k)).
//!
//! The analysis lives in [`crate::domain::getf_default_nil_report`], which also backs the
//! standalone `inspect getf-default-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::getf_default_nil_report::examine;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "getf-default-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a getf call with an explicit nil default, the default ((getf p k nil) is (getf p k))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("getf")];

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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant nil default".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit nil default restates getf's default; (getf p k nil) is (getf p k)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
