//! `defpackage-quoted`: a quoted designator in a defpackage clause, which defpackage does not evaluate ((:export 'foo)).
//!
//! The analysis lives in [`crate::defpackage_quoted::domain`], which also backs the
//! standalone `inspect defpackage-quoted` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::defpackage_quoted::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "defpackage-quoted",
    RuleCategory::Malformed,
    Severity::Warning,
    "a quoted designator in a defpackage clause, which defpackage does not evaluate ((:export 'foo))",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defpackage")];

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
        let mut defpackage_form_count = 0;
        let mut items = Vec::new();
        examine(view, context.path(), &mut defpackage_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "defpackage does not evaluate its options; drop the quote in the {} clause",
                    item.clause
                ),
            );
        }
        Ok(())
    }
}
