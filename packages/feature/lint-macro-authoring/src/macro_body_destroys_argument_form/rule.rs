//! `macro-body-destroys-argument-form`: a macro expander applying a destructive
//! operator directly to one of its own parameters.
//!

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::macro_body_destroys_argument_form::domain::examine_macro_body_destroys_argument_form;

pub const META: RuleMeta = RuleMeta::new(
    "macro-body-destroys-argument-form",
    RuleCategory::Suspicious,
    // The program is silently rewritten. SBCL emits nothing at all, and the
    // second expansion of the call site produces a different program.
    Severity::Error,
    "a macro expander applying a destructive operator to its own parameter",
    // The repair is to copy — but *where* to copy is the author's decision:
    // wrapping the argument in `copy-list` is right for a list and wrong for a
    // vector, and the non-destructive operator is often the better fix.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A macro's parameters are bound to the caller's own source structure: `&body` is a tail \
         of the list the reader built for the call site. A destructive operator applied to one \
         edits the program in place, so the next expansion of that call site sees the edited \
         version and produces different code.",
    )
    .with_example(
        "(defmacro bad (&body forms) `(progn ,@(nreverse forms)))",
        "(defmacro ok (&body forms) `(progn ,@(reverse forms)))",
    )
    .with_caveat(
        "A parameter the expander rebinds or reassigns is not reported: `(let ((body (copy-list \
         body))) …)` makes the list the expander's own, and this rule is not flow-sensitive.",
    ),
);

/// The two definitions whose parameters are bound to caller source.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut definition_count = 0;
        let mut items = Vec::new();
        examine_macro_body_destroys_argument_form(
            context.tree(),
            view,
            &mut definition_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
