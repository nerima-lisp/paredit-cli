//! `duplicate-lambda-list-keyword`: a lambda list that repeats a lambda-list keyword (&optional, &key, ...).
//!
//! The analysis lives in [`crate::domain::duplicate_lambda_list_keyword_report`], which also backs the
//! standalone `inspect duplicate-lambda-list-keyword` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::duplicate_lambda_list_keyword_report::collect_duplicate_lambda_list_keywords;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-lambda-list-keyword",
    RuleCategory::Duplicate,
    Severity::Error,
    "a lambda list that repeats a lambda-list keyword (&optional, &key, ...)",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let (_, items) = collect_duplicate_lambda_list_keywords(
            context.path(),
            context.dialect(),
            context.tree(),
        )?;
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} repeats lambda-list keyword {} ({}×)",
                    item.definition, item.keyword, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
