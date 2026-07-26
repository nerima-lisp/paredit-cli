//! `negated-when-unless`: a when/unless whose test is a (not X)/(null X) negation (flip the macro instead).
//!
//! The analysis lives in [`crate::domain::negated_when_unless_report`], which also backs the
//! standalone `inspect negated-when-unless` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::negated_when_unless_report::examine_conditional;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "negated-when-unless",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a when/unless whose test is a (not X)/(null X) negation (flip the macro instead)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("when"), NormalizedHead::new("unless")];

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
        let mut conditional_form_count = 0;
        let mut items = Vec::new();
        examine_conditional(
            view,
            context.path(),
            &mut conditional_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Two disjoint edits: flip the head macro and drop the negation,
                // leaving the body and all spacing byte-identical.
                let fix_first = Replacement::new(item.head_span, item.suggested_head.to_owned());
                let fix_rest = [Replacement::new(
                    item.test_span,
                    context_slice(item.inner_span),
                )];

                RuleFix::multi(
                    format!(
                        "Rewrite {} ({} …) as {}",
                        item.head, item.negator, item.suggested_head
                    ),
                    fix_first,
                    fix_rest,
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} test is ({} …); use {} on the un-negated test",
                    item.head, item.negator, item.suggested_head
                ),
                fix,
            );
        }
        Ok(())
    }
}
