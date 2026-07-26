//! `nthcdr-small-index`: an nthcdr with a 1-4 count that has a named cdr accessor ((nthcdr 2 x) is (cddr x)).
//!
//! The analysis lives in [`crate::domain::nthcdr_small_index_report`], which also backs the
//! standalone `inspect nthcdr-small-index` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nthcdr_small_index_report::examine;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nthcdr-small-index",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an nthcdr with a 1-4 count that has a named cdr accessor ((nthcdr 2 x) is (cddr x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("nthcdr")];

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
        let mut nthcdr_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut nthcdr_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // (nthcdr 2 x) is (cddr x): rewrite to the named cdr accessor.
                let text = format!("({} {})", item.accessor, context_slice(item.list_span));

                RuleFix::single(
                    item.span,
                    text,
                    format!("Rewrite (nthcdr n …) as ({} …)", item.accessor),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "nthcdr with a small count has a named cdr accessor; use ({} …)",
                    item.accessor
                ),
                fix,
            );
        }
        Ok(())
    }
}
