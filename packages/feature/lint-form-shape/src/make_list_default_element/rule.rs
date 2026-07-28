//! `make-list-default-element`: a make-list call with an explicit :initial-element nil, the default ((make-list n :initial-element nil) is (make-list n)).
//!
//! The analysis lives in [`crate::make_list_default_element::domain`], which also backs the
//! standalone `inspect make-list-default-element` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::make_list_default_element::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "make-list-default-element",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a make-list call with an explicit :initial-element nil, the default ((make-list n :initial-element nil) is (make-list n))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-list")];

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
    ) -> LintResult<()> {
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                RuleFix::multi(
                    "Drop the redundant :initial-element nil".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                "explicit :initial-element nil restates make-list's default; drop it".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
