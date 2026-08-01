//! `self-recursive-tail-call`: a function's own name called in tail position
//! of its own body, annotated with whether the target dialect guarantees
//! tail-call optimization there.
//!
//! The analysis lives in [`crate::self_recursive_tail_call::domain`], which
//! also backs the standalone `inspect self-recursive-tail-call` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::self_recursive_tail_call::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "self-recursive-tail-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a function's own name called in tail position of its body, annotated with the target \
     dialect's tail-call guarantee",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "A self-recursive call in tail position is a loop written as a function call, and \
         whether that is safe is a fact about the dialect, not the code. Scheme, Racket, LFE, and \
         Fennel guarantee a proper tail call here; Emacs Lisp, Hy, and Clojure do not; Common Lisp \
         leaves it to the implementation.",
    )
    .with_example(
        "(defun count-down (n) (if (zerop n) 'done (count-down (1- n))))",
        "(defun count-down (n) (loop for i downfrom n to 0 finally (return 'done)))",
    )
    .with_caveat(
        "Janet and Carp are not modeled: this rule does not assert a fact about their tail-call \
         behaviour it cannot verify. A self-recursive call buried inside another call's arguments \
         is not in tail position and is not reported.",
    ),
);

/// Every dialect this tool parses except `Unknown`, for which a tail-call
/// fact would not mean anything.
const SCOPE: [Dialect; 10] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Lfe,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::Clojure,
    Dialect::Hy,
    Dialect::Carp,
    Dialect::Janet,
    Dialect::Fennel,
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // A definition can be a top-level `defun` or a local `labels`/`flet`
        // function; every node is checked for the shape.
        HeadFilter::AllNodes
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&SCOPE)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            context.dialect(),
            &mut scanned_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "{} calls itself here in tail position: {}",
                    item.name,
                    item.tco_status.explanation()
                ),
            );
        }
        Ok(())
    }
}
