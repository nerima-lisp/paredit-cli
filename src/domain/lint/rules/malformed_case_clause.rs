//! `malformed-case-clause`: a case/typecase clause that is not a non-empty list.
//!
//! The analysis lives in [`crate::domain::malformed_case_clause_report`], which also backs the
//! standalone `inspect malformed-case-clause` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::malformed_case_clause_report::examine_case;
use crate::domain::sexpr::ExpressionView;

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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let mut case_form_count = 0;
        let mut items = Vec::new();
        examine_case(view, context.path(), &mut case_form_count, &mut items);
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
