//! `nil-comparison`: an eq/eql/equal/equalp comparison against nil ((eq x nil) is just (null x)).
//!
//! The analysis lives in [`crate::domain::nil_comparison_report`], which also backs the
//! standalone `inspect nil-comparison` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::nil_comparison_report::examine_comparison;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nil-comparison",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an eq/eql/equal/equalp comparison against nil ((eq x nil) is just (null x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("eq"),
    NormalizedHead::new("eql"),
    NormalizedHead::new("equal"),
    NormalizedHead::new("equalp"),
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
        let mut comparison_form_count = 0;
        let mut items = Vec::new();
        examine_comparison(view, context.path(), &mut comparison_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Rewrite the whole form as `(null X)`, copying X's exact source.

                RuleFix::single(
                    item.span,
                    format!("(null {})", context_slice(item.operand_span)),
                    format!("Rewrite ({} X nil) as (null X)", item.operator),
                )
            };

            sink.report_fixed(
                span,
                format!("{} against nil is a null test; use (null X)", item.operator),
                fix,
            );
        }
        Ok(())
    }
}
