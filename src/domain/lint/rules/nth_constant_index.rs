//! `nth-constant-index`: an nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x)).
//!
//! The analysis lives in [`crate::domain::nth_constant_index_report`], which also backs the
//! standalone `inspect nth-constant-index` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nth_constant_index_report::examine_nth;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nth-constant-index",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an nth with a small constant index that has an ordinal accessor ((nth 0 x) is (first x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("nth")];

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
        let mut nth_form_count = 0;
        let mut items = Vec::new();
        examine_nth(view, context.path(), &mut nth_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Rewrite `(nth N x)` as `(ordinal x)`, copying the list source.

                RuleFix::single(
                    item.span,
                    format!("({} {})", item.ordinal, context_slice(item.list_span)),
                    format!(
                        "Use ({} …) instead of nth with a constant index",
                        item.ordinal
                    ),
                )
            };

            sink.report_fixed(
                span,
                format!("nth with a constant index; use ({} …)", item.ordinal),
                fix,
            );
        }
        Ok(())
    }
}
