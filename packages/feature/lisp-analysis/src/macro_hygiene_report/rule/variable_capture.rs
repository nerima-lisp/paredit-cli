//! `macro-variable-capture`: a template binds a name the caller can see.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_hygiene_report::domain::HygieneRisk;

pub const META: RuleMeta = RuleMeta::new(
    "macro-variable-capture",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a defmacro template binds a literal name that is not obviously a gensym",
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
            if finding.risk != HygieneRisk::VariableCapture {
                continue;
            }
            // The remedy rides in the message for the same reason it does in
            // `multiple_evaluation`: the sink takes a rule, a span and a
            // string, and has no `remedy` column to put it in.
            let message = format!(
                "variable capture: macro `{}` binds `{}` to a literal name that is not \
                 obviously a gensym; {}",
                finding.macro_name, finding.subject, finding.remedy
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
