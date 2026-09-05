//! `typecase-nil-key`: a typecase/etypecase/ctypecase clause with a bare nil type, which is the empty type and never matches (use null).
//!

use paredit_core_lint_engine::LintResult;

use crate::typecase_nil_key::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "typecase-nil-key",
    RuleCategory::DeadCode,
    Severity::Error,
    "a typecase/etypecase/ctypecase clause with a bare nil type, which is the empty type and never matches (use null)",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("typecase"),
    NormalizedHead::new("etypecase"),
    NormalizedHead::new("ctypecase"),
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
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut typecase_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, &mut typecase_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} clause type nil is the empty type and never matches; use null",
                    item.head
                ),
            );
        }
        Ok(())
    }
}
