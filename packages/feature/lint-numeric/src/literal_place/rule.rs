//! `literal-place`: an incf/decf/push/pop/pushnew/setf/psetf whose place is a self-evaluating literal (cannot be modified).
//!
//! The analysis lives in [`crate::literal_place::domain`], which also backs the
//! standalone `inspect literal-place` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::literal_place::domain::examine_modify;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "literal-place",
    RuleCategory::Malformed,
    Severity::Error,
    "an incf/decf/push/pop/pushnew/setf/psetf whose place is a self-evaluating literal (cannot be modified)",
    Fixability::ReportOnly,
);

/// The seven modify-macro heads `examine_modify` accepts.
const HEADS: [NormalizedHead; 7] = [
    NormalizedHead::new("incf"),
    NormalizedHead::new("decf"),
    NormalizedHead::new("pop"),
    NormalizedHead::new("push"),
    NormalizedHead::new("pushnew"),
    NormalizedHead::new("setf"),
    NormalizedHead::new("psetf"),
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
    ) -> LintResult<()> {
        let mut modify_form_count = 0;
        let mut items = Vec::new();
        examine_modify(view, context.source(), &mut modify_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} place {} is a literal and cannot be modified",
                    item.operator, item.place
                ),
            );
        }
        Ok(())
    }
}
