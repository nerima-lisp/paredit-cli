//! `elisp-macro-missing-declare`: an Emacs Lisp macro with no editor
//! declaration.
//!
//! Categorised `Malformed` rather than `Suspicious` alongside the other four:
//! this is a definition form omitting an optional-but-expected option, which
//! is what `elisp-defcustom-missing-type` and `elisp-defcustom-missing-group`
//! already report, and not code whose meaning is other than it looks.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_hygiene_report::domain::HygieneRisk;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-macro-missing-declare",
    RuleCategory::Malformed,
    Severity::Warning,
    "an Emacs Lisp macro with no leading (declare (indent ...)) or (declare (debug ...))",
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
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        for finding in super::hygiene_findings(context, view) {
            if finding.risk != HygieneRisk::MissingEditorDeclaration {
                continue;
            }
            // Prefixed with the risk label, like the four `macro-*` siblings,
            // so a reader scanning a mixed report sees which of the five
            // risks a line is about before reading the rest of it.
            let message = format!(
                "missing editor declaration: macro `{}` is defined without one; {}",
                finding.macro_name, finding.remedy
            );
            sink.report(finding.span, message);
        }
        Ok(())
    }
}
