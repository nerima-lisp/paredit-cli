//! `elisp-save-excursion-set-buffer`: `save-excursion` wrapping `set-buffer`.
//!
//! The reason usually given for this rule is that `save-excursion` stopped
//! restoring the current buffer in Emacs 25.1. That is **false**, and this
//! rule does not say it. Measured in GNU Emacs 31.0.91:
//!
//! ```text
//! (with-temp-buffer
//!   (let ((a (current-buffer)))
//!     (get-buffer-create "zz")
//!     (save-excursion (set-buffer "zz"))
//!     (eq a (current-buffer))))          ; => t
//! ```
//!
//! and the docstring's own first line is "Save point, and current buffer".
//! What Emacs 25.1 removed was the saving of the **mark**, which is why
//! `save-mark-and-excursion` exists.
//!
//! The real complaint is the one Emacs itself makes. Byte-compiling
//! `(save-excursion (set-buffer b) …)` produces:
//!
//! ```text
//! Warning: Use ‘with-current-buffer’ rather than save-excursion+set-buffer
//! ```
//!
//! `save-excursion` saves point *in whichever buffer is current when it runs*,
//! so the pair saves point in the old buffer and then leaves it — the point
//! protection the code appears to be asking for is not the one it gets.
//! `with-current-buffer` says the intent directly, and this rule reports the
//! pair for a reader who is not byte-compiling.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-save-excursion-set-buffer",
    RuleCategory::Suspicious,
    Severity::Warning,
    "save-excursion wrapping set-buffer, which saves point in the buffer it then leaves",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`save-excursion` saves point in whichever buffer is current when it runs, so wrapping a \
         `set-buffer` saves point in the buffer being left rather than the one being worked in. \
         The current buffer *is* restored — that part has not changed — but the point protection \
         the form appears to ask for is not the one it provides. Emacs's own byte compiler emits \
         `Use 'with-current-buffer' rather than save-excursion+set-buffer` for this exact pair.",
    )
    .with_example(
        "(save-excursion (set-buffer buf) (goto-char (point-min)))",
        "(with-current-buffer buf (goto-char (point-min)))",
    )
    .with_caveat(
        "A `set-buffer` inside a nested `with-current-buffer`, `save-current-buffer`, \
         `with-temp-buffer`, another `save-excursion`, or a nested function definition belongs to \
         that form and is not reported against the outer one.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("save-excursion")];

/// Forms that establish their own current-buffer scope, or their own body
/// entirely. A `set-buffer` under one of these is not the outer
/// `save-excursion`'s problem.
const OPAQUE: [&str; 8] = [
    "save-excursion",
    "save-current-buffer",
    "with-current-buffer",
    "with-temp-buffer",
    "with-output-to-temp-buffer",
    "lambda",
    "defun",
    "defmacro",
];

/// The first `(set-buffer …)` in `body` that belongs to this `save-excursion`.
///
/// A hand-rolled stack rather than recursion: a rule must not blow the stack
/// on a deeply nested but perfectly legal file.
fn buffer_switch_in(body: &[ExpressionView]) -> Option<&ExpressionView> {
    let mut stack: Vec<&ExpressionView> = body.iter().rev().collect();
    while let Some(view) = stack.pop() {
        match list_head(view) {
            Some("set-buffer") => return Some(view),
            Some(head) if OPAQUE.contains(&head) => continue,
            _ => stack.extend(view.children.iter().rev()),
        }
    }
    None
}

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
        if list_head(view) != Some("save-excursion") {
            return Ok(());
        }
        let Some(body) = view.children.get(1..) else {
            return Ok(());
        };
        let Some(switch) = buffer_switch_in(body) else {
            return Ok(());
        };
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        sink.report(
            switch.span,
            "save-excursion saves point in the buffer this set-buffer then \
             leaves; `with-current-buffer` is the form that says what this \
             means"
                .to_owned(),
        );
        Ok(())
    }
}
