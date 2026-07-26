//! `redundant-quote`: a self-evaluating literal (number/string/char/keyword) quoted redundantly.
//!
//! The analysis lives in [`crate::domain::redundant_quote_report`], which also backs the
//! standalone `inspect redundant-quote` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_quote_report::examine_quote;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-quote",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a self-evaluating literal (number/string/char/keyword) quoted redundantly",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// A redundant quote is reader sugar on *any* node — `'5` is a quoted atom
    /// — so this cannot be narrowed to a set of operator heads.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::AllNodes
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let mut quoted_form_count = 0;
        let mut items = Vec::new();
        examine_quote(view, context.path(), &mut quoted_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::single(
                    item.span,
                    item.literal,
                    "Remove the redundant quote".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!("quoting {} {} is redundant", item.kind, item.literal),
                fix,
            );
        }
        Ok(())
    }
}
