//! `accessor-arity`: an nth/elt/gethash/getf/... accessor with the wrong number of arguments.
//!
//! The analysis lives in [`crate::accessor_arity::domain`], which also backs the
//! standalone `inspect accessor-arity` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::accessor_arity::domain::examine_call;
use crate::accessor_arity::domain::expected_arity_phrase as accessor_arity_phrase;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "accessor-arity",
    RuleCategory::Arity,
    Severity::Error,
    "an nth/elt/gethash/getf/... accessor with the wrong number of arguments",
    Fixability::ReportOnly,
);

/// Every accessor `examine_call` recognizes: the binary element accessors plus
/// the two/three-argument keyed lookups.
const HEADS: [NormalizedHead; 8] = [
    NormalizedHead::new("nth"),
    NormalizedHead::new("elt"),
    NormalizedHead::new("nthcdr"),
    NormalizedHead::new("svref"),
    NormalizedHead::new("char"),
    NormalizedHead::new("schar"),
    NormalizedHead::new("gethash"),
    NormalizedHead::new("getf"),
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
        let mut call_count = 0;
        let mut items = Vec::new();
        examine_call(view, context.path(), &mut call_count, &mut items);
        for item in items {
            let span = item.span;
            let expected = accessor_arity_phrase(&item);

            sink.report(
                span,
                format!(
                    "{} takes {} argument(s) but has {}",
                    item.operator, expected, item.argument_count
                ),
            );
        }
        Ok(())
    }
}
