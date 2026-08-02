//! `lambda-list-keyword-order`: lambda-list keywords out of the canonical &optional/&rest/&key/&aux order.
//!
//! The analysis lives in [`crate::lambda_list_keyword_order::domain`], which also backs the
//! standalone `inspect lambda-list-keyword-order` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::lambda_list_keyword_order::domain::collect_lambda_list_keyword_order_violations;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "lambda-list-keyword-order",
    RuleCategory::Malformed,
    Severity::Error,
    "lambda-list keywords out of the canonical &optional/&rest/&key/&aux order",
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
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let (violations, _definition_count) =
            collect_lambda_list_keyword_order_violations(context.dialect(), view);
        for item in violations {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} lists lambda-list keyword {} after {}",
                    item.definition, item.keyword, item.after_keyword
                ),
            );
        }
        Ok(())
    }
}
