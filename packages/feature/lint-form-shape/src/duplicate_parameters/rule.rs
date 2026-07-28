//! `duplicate-parameters`: a lambda list that names the same parameter more than once.
//!
//! The analysis lives in [`crate::duplicate_parameters::domain`], which also backs the
//! standalone `inspect duplicate-parameters` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::duplicate_parameters::domain::collect_duplicate_parameters;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-parameters",
    RuleCategory::Duplicate,
    Severity::Error,
    "a lambda list that names the same parameter more than once",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        _view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let (_, items) =
            collect_duplicate_parameters(context.path(), context.dialect(), context.tree())?;
        for item in items {
            let span = item.span;

            sink.report(
                span,
                format!(
                    "{} names parameter {} more than once ({}×)",
                    item.definition, item.parameter, item.occurrence_count
                ),
            );
        }
        Ok(())
    }
}
