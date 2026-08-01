//! `macro-deep-quasiquote-nesting`: three or more nested quasiquote levels.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_hygiene_report::domain::HygieneRisk;

pub const META: RuleMeta = RuleMeta::new(
    "macro-deep-quasiquote-nesting",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a macro template nests three or more quasiquote levels",
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
            if finding.risk != HygieneRisk::DeepQuasiquoteNesting {
                continue;
            }
            // Named by the macro rather than by a subject: the whole template
            // is at fault here, so the domain leaves `subject` empty.
            //
            // The domain's `remedy` opens with "three or more nested backquote
            // levels", which the concrete depth just above already says. Only
            // its actionable half is carried over.
            let message = format!(
                "deep quasiquote nesting: macro `{}` nests {} quasiquote levels, which is easy \
                 to get an escape wrong in; consider splitting it into helper macros or \
                 functions to reduce the nesting",
                finding.macro_name, finding.occurrences
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
