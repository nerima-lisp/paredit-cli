//! `elisp-condition-case-without-handler`: a `condition-case` that handles
//! nothing.
//!
//! `(condition-case err FORM)` with no handler clauses evaluates `FORM` and
//! nothing else. It is not an error, it does not catch anything, and it reads
//! at a glance exactly like a form that does — which is what makes it worth
//! reporting. Usually a handler was deleted, or the clauses were written
//! outside the form by accident.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::emacs_lisp_operator;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-condition-case-without-handler",
    RuleCategory::Suspicious,
    Severity::Error,
    "a condition-case with no handler clauses, which catches nothing",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("condition-case"),
    NormalizedHead::new("condition-case-unless-debug"),
];

/// `(condition-case VAR BODYFORM HANDLERS…)`: the first handler sits at 3.
const FIRST_HANDLER: usize = 3;

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::EMACS_LISP_ONLY
    }

    fn check(
        &self,
        _context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        if !matches!(
            emacs_lisp_operator(view),
            Some(EmacsLispOperator::ConditionCase | EmacsLispOperator::ConditionCaseUnlessDebug)
        ) {
            return Ok(());
        }
        // A form with no body form at all is malformed rather than handlerless,
        // and saying so is another rule's job.
        if view.children.len() < FIRST_HANDLER {
            return Ok(());
        }
        if view.children.len() > FIRST_HANDLER {
            return Ok(());
        }

        sink.report(
            view.span,
            "this condition-case has no handler clauses, so it evaluates its \
             body form and catches nothing",
        );
        Ok(())
    }
}
