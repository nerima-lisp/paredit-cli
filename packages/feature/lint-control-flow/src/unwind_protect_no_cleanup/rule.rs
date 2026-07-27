//! `unwind-protect-no-cleanup`: an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x).
//!
//! The analysis lives in [`crate::domain::unwind_protect_no_cleanup_report`], which also backs the
//! standalone `inspect unwind-protect-no-cleanup` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;
use crate::domain::unwind_protect_no_cleanup_report::examine;

pub const META: RuleMeta = RuleMeta::new(
    "unwind-protect-no-cleanup",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an unwind-protect with no cleanup forms, which is just its body ((unwind-protect x) is x)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("unwind-protect")];

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
        let mut unwind_protect_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            &mut unwind_protect_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // (unwind-protect x) is x.

                RuleFix::single(
                    item.span,
                    context_slice(item.form_span),
                    "Drop the cleanupless unwind-protect".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "an unwind-protect with no cleanup is just its body; (unwind-protect x) is x"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
