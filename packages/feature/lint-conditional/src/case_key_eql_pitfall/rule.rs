//! `case-key-eql-pitfall`: a case clause keyed on a string or float literal.
//!
//! The analysis lives in [`crate::case_key_eql_pitfall::domain`], which also
//! backs the standalone `inspect case-key-eql-pitfall` command; this module
//! only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::case_key_eql_pitfall::domain::examine_case;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "case-key-eql-pitfall",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a case/ecase/ccase clause keyed on a string or float literal, which eql does not match dependably",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("ccase"),
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
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, &mut case_form_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once for the whole form rather than once per key: every key
        // reported here belongs to this one `case`, so they share its verdict.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let message = format!(
                "{} never matches dependably: {}",
                item.key,
                item.pitfall.reason()
            );
            sink.report(item.span, message);
        }
        Ok(())
    }
}
