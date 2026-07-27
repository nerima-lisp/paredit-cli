//! `duplicate-keyword`: a make-* call passing the same keyword argument twice ((make-instance 'c :x 1 :x 2)).
//!
//! The analysis lives in [`crate::duplicate_keyword::domain`], which also backs the
//! standalone `inspect duplicate-keyword` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::duplicate_keyword::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-keyword",
    RuleCategory::Duplicate,
    Severity::Warning,
    "a make-* call passing the same keyword argument twice ((make-instance 'c :x 1 :x 2))",
    Fixability::ReportOnly,
);

/// Every operator `examine` recognizes as having a fixed positional-argument
/// prefix (see [`crate::duplicate_keyword::domain::KEY_OPERATORS`]).
const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("make-instance"),
    NormalizedHead::new("make-hash-table"),
    NormalizedHead::new("make-array"),
    NormalizedHead::new("make-string"),
    NormalizedHead::new("make-condition"),
    NormalizedHead::new("make-pathname"),
    NormalizedHead::new("make-string-output-stream"),
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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut call_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "keyword {} is passed more than once; the leftmost value wins",
                    item.keyword
                ),
            );
        }
        Ok(())
    }
}
