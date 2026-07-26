//! `redundant-let-star`: a let* with zero or one binding, which is just let (no sequential scope in play).
//!
//! The analysis lives in [`crate::domain::redundant_let_star_report`], which also backs the
//! standalone `inspect redundant-let-star` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_let_star_report::examine_let_star;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-let-star",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let* with zero or one binding, which is just let (no sequential scope in play)",
    Fixability::Fixable,
);

/// `examine_let_star` only ever matches a `let*` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("let*")];

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
        let mut let_star_form_count = 0;
        let mut items = Vec::new();
        examine_let_star(view, context.path(), &mut let_star_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Rewrite just the head symbol: a ≤1-binding let* is exactly let.

                RuleFix::single(
                    item.head_span,
                    "let".to_owned(),
                    "Rewrite the redundant let* as let".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "let* with {} binding{} is just let; sequential scope is unused",
                    item.binding_count,
                    if item.binding_count == 1 { "" } else { "s" }
                ),
                fix,
            );
        }
        Ok(())
    }
}
