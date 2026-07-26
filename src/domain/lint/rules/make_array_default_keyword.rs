//! `make-array-default-keyword`: a make-array call with an explicit :adjustable nil or :fill-pointer nil, the default ((make-array n :adjustable nil) is (make-array n)).
//!
//! The analysis lives in [`crate::domain::make_array_default_keyword_report`], which also backs the
//! standalone `inspect make-array-default-keyword` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::make_array_default_keyword_report::examine;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "make-array-default-keyword",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a make-array call with an explicit :adjustable nil or :fill-pointer nil, the default ((make-array n :adjustable nil) is (make-array n))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-array")];

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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();

                RuleFix::multi(
                    format!("Drop the redundant {} nil", item.keyword),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "explicit {} nil restates make-array's default; drop it",
                    item.keyword
                ),
                fix,
            );
        }
        Ok(())
    }
}
