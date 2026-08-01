//! `macro-parameter-reordering`: the expansion evaluates the caller's
//! argument forms in an order the call site does not read.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_hygiene_report::domain::HygieneRisk;

pub const META: RuleMeta = RuleMeta::new(
    "macro-parameter-reordering",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a macro template unquotes its parameters in an order the lambda list does not",
    Fixability::ReportOnly,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        super::head_filter()
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        super::dialect_scope()
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for finding in super::hygiene_findings(context, view) {
            if finding.risk != HygieneRisk::ParameterReordering {
                continue;
            }
            // The domain's `remedy` opens by restating the problem, which the
            // report renders in its own column but which would read as a
            // second clause of the same sentence here. Only its actionable
            // half is carried over, so the message stays one sentence.
            let message = format!(
                "parameter reordering: macro `{}` unquotes its parameters as `{}`, which is \
                 not the order its lambda list writes them, so reorder the template or bind \
                 each argument in the call's own order before using it",
                finding.macro_name, finding.subject
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
