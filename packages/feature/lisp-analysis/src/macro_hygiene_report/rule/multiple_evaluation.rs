//! `macro-multiple-evaluation`: a form the expansion runs more than once —
//! either an argument form the template unquotes twice, or a `symbol-macrolet`
//! expansion referenced twice.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_hygiene_report::domain::HygieneRisk;

pub const META: RuleMeta = RuleMeta::new(
    "macro-multiple-evaluation",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a macro expansion runs one form more than once: an argument form unquoted twice, or a \
     symbol-macrolet expansion referenced twice",
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
            if finding.risk != HygieneRisk::MultipleEvaluation {
                continue;
            }
            // The remedy is carried in the message because a lint finding has
            // nowhere else to put it: the sink takes a rule, a span and a
            // string, while the standalone report has a `remedy` column.
            let message = format!(
                "multiple evaluation: `{}` is evaluated {} times; {}",
                finding.subject, finding.occurrences, finding.remedy
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
