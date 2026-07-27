//! `duplicate-lambda-list-keyword`: a lambda list that repeats a lambda-list keyword (&optional, &key, ...).
//!
//! The analysis lives in [`crate::duplicate_lambda_list_keyword::domain`], which also backs the
//! standalone `inspect duplicate-lambda-list-keyword` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::duplicate_lambda_list_keyword::domain::collect_duplicate_lambda_list_keywords;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

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
    ) -> LintResult<()> {
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
