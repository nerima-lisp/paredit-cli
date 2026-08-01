//! `dolist-result-form-references-loop-variable`: a `dolist` result form
//! reading the loop variable, which the standard binds to `nil` there.
//!
//! The analysis lives in
//! [`crate::dolist_result_form_references_loop_variable::domain`], which also
//! backs the standalone `inspect dolist-result-form-references-loop-variable`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::dolist_result_form_references_loop_variable::domain::examine_dolist_result;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "dolist-result-form-references-loop-variable",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a dolist result form reading the loop variable, which the spec binds to nil there",
    // The value the author wanted is nowhere in the form: substituting `nil`
    // would preserve behaviour while hiding the bug, and anything else would
    // change what the `dolist` returns.
    Fixability::ReportOnly,
);

/// `examine_dolist_result` only ever matches a `dolist` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("dolist")];

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
        let mut dolist_form_count = 0;
        let mut items = Vec::new();
        examine_dolist_result(view, &mut dolist_form_count, &mut items);
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The engine's head index is what keeps this rule off every file with no
    /// `dolist` form in it. `AllNodes` would cost one call per node and
    /// `WholeTree` one pass per file, both of them paid even when the rule
    /// matches nothing — which is precisely what the `clean/forms/*`
    /// benchmarks measure. Pinned here so the declaration cannot drift.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["dolist"]);
    }
}
