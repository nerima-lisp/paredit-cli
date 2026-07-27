//! `lambda-list-keyword-order`: lambda-list keywords out of the canonical &optional/&rest/&key/&aux order.
//!
//! The analysis lives in [`crate::domain::lambda_list_keyword_order_report`], which also backs the
//! standalone `inspect lambda-list-keyword-order` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lambda_list_keyword_order_report::collect_lambda_list_keyword_order;
use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use crate::domain::lint::rule::LintRule;
use crate::domain::sexpr::ExpressionView;

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
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let (_, items) =
            collect_lambda_list_keyword_order(context.path(), context.dialect(), context.tree())?;
        for item in items {
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
