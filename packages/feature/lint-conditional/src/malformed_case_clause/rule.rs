//! `malformed-case-clause`: a case/typecase clause that is not a non-empty list.
//!

use paredit_core_lint_engine::LintResult;

use crate::malformed_case_clause::domain::examine_case;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "malformed-case-clause",
    RuleCategory::Malformed,
    Severity::Error,
    "a case/typecase clause that is not a non-empty list",
    Fixability::ReportOnly,
);

/// Every head `examine_case` accepts: the `case`-family forms whose clauses
/// must be non-empty lists.
const HEADS: [NormalizedHead; 6] = [
    NormalizedHead::new("case"),
    NormalizedHead::new("ccase"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("typecase"),
    NormalizedHead::new("ctypecase"),
    NormalizedHead::new("etypecase"),
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
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, &mut case_form_count, &mut items);
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} clause {} is not a non-empty list",
                    item.head, item.clause
                ),
            );
        }
        Ok(())
    }
}
