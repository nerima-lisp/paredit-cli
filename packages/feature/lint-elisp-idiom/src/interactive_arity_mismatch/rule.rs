//! `elisp-interactive-arity-mismatch`: a command whose interactive spec
//! supplies fewer arguments than its lambda list requires.
//!
//! `(interactive)` with no descriptor passes *no* arguments, and a string
//! descriptor passes one per newline-separated code letter. Neither is checked
//! against the lambda list at any point before the command runs. Verified
//! against GNU Emacs 31.0.91:
//!
//! - `(defun f (a) (interactive) …)` — `(commandp 'f)` is `t`, and
//!   `(call-interactively 'f)` signals `(wrong-number-of-arguments … 0)`.
//! - `(defun f (a b) (interactive "p") …)` — the same signal.
//! - byte-compiling a file containing either produces no warning at all.
//!
//! So the definition looks like a command, installs like a command, and
//! breaks the first time anybody invokes it.
//!
//! Everything from `&optional` on is left out of the count: those arguments
//! default to nil when the spec supplies nothing, which is exactly the idiom
//! `(defun f (&optional arg) (interactive "P") …)` relies on.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{
    Definition, emacs_lisp_operator, interactive_argument_count, is_unevaluated_at,
};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-interactive-arity-mismatch",
    RuleCategory::Arity,
    Severity::Error,
    "an interactive spec that supplies fewer arguments than the lambda list requires",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`call-interactively` passes exactly the arguments the `(interactive …)` descriptor names \
         — none at all when there is no descriptor, one per newline-separated code letter when it \
         is a string. A required parameter left unsupplied is a `wrong-number-of-arguments` \
         signal at the first invocation, and nothing before that reports it.",
    )
    .with_example(
        "(defun my-go (n) (interactive) (forward-line n))",
        "(defun my-go (n) (interactive \"p\") (forward-line n))",
    )
    .with_caveat(
        "A descriptor that is not a literal string is evaluated to produce the argument list, so \
         `(interactive (list a b))` is never reported: what it supplies cannot be read from the \
         source.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defsubst"),
    NormalizedHead::new("cl-defun"),
    NormalizedHead::new("cl-defsubst"),
];

/// The same heads in the spelling `EmacsLispOperator::from_head` reads, for the
/// test that pins the invariant `check` relies on.
#[cfg(test)]
pub(crate) const HEAD_NAMES: [&str; 4] = ["defun", "defsubst", "cl-defun", "cl-defsubst"];

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
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(operator) = emacs_lisp_operator(view) else {
            return Ok(());
        };
        // Only the function definitions reach here: `HEADS` lists no macro
        // form, and `every_head_here_is_a_function_definition` is the test
        // that keeps that true. A macro cannot be a command at all, which is
        // `elisp-interactive-in-macro`'s complaint rather than this rule's.
        let Some(shape) = operator.callable_shape() else {
            return Ok(());
        };

        let definition = Definition { view, shape };
        let Some(header) = definition.interactive_header() else {
            return Ok(());
        };
        let Some(supplied) = interactive_argument_count(header) else {
            return Ok(());
        };
        let required = definition.required_argument_count();
        if required <= supplied {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        let name = definition.name();
        sink.report(
            header.span,
            format!(
                "{name} requires {required} argument(s) but its interactive \
                 spec supplies {supplied}, so calling it interactively signals \
                 wrong-number-of-arguments"
            ),
        );
        Ok(())
    }
}
