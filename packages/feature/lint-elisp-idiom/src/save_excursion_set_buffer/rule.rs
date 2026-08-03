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

// A list of scope-establishing forms (`with-current-buffer`,
// `save-current-buffer`, `with-temp-buffer`, `lambda`, …) used to live here, to
// stop the body walk descending into a `set-buffer` that belonged to some inner
// form. The walk is gone — see `buffer_switch_in` — and with it the need for the
// list: a nested scope cannot simultaneously *be* the first body form and
// *contain* the `set-buffer`, so every exclusion it bought is now structural.
// The rule's caveat still documents the behaviour, because the behaviour has not
// changed.

/// The `(set-buffer …)` that opens `body`, if that is how it opens.
///
/// Deliberately only the **first** body form, which is exactly what Emacs's own
/// `byte-compile-save-excursion` tests — it reads
/// `(car-safe (car-safe (cdr form)))` and nothing deeper. Matching the compiler
/// is the whole justification for this rule, so matching it loosely is worse
/// than not matching it at all.
///
/// An earlier version walked the entire body. Over the 1654 `.el` files GNU
/// Emacs 30.2 ships that produced 227 findings and **not one** was the
/// first-body-form case: Emacs fixed those decades ago precisely because the
/// compiler warns about them. What it reported instead was code like
/// `(save-excursion (goto-char (point-min)) (when flag (set-buffer buf) …))`,
/// where the saved point is used before the switch and the finding's own
/// message — that the form "saves point in a buffer it then leaves" — is false.
///
/// `OPAQUE` is therefore no longer consulted here; it remains documented in the
/// rule's caveat because the narrowed rule inherits the same exclusions for
/// free (a nested scope cannot be the first body form and a `set-buffer` at
/// once).
fn buffer_switch_in(body: &[ExpressionView]) -> Option<&ExpressionView> {
    let first = body.first()?;
    (list_head(first) == Some("set-buffer")).then_some(first)
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
