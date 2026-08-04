//! `unwind-protect-cleanup-signals`: a cleanup that destroys the condition it
//! was unwinding for.
//!
//! `unwind-protect` runs its cleanup forms however the protected form leaves —
//! normally, or by a non-local exit. When the exit is a condition being
//! signalled, the cleanup runs *during* the unwind, and a cleanup that signals
//! an error of its own replaces the one already in flight. The original is not
//! chained, not suppressed, not logged. It is gone.
//!
//! # What SBCL 2.6.0 actually does
//!
//! ```text
//! (handler-case (unwind-protect (error "ORIGINAL") (error "CLEANUP"))
//!   (error (c) (format t "ESCAPED: ~A~%" c)))
//! => ESCAPED: CLEANUP
//!
//! ;; control, with a cleanup that does not signal:
//! (handler-case (unwind-protect (error "ORIGINAL") (format t "cleaning~%"))
//!   (error (c) (format t "ESCAPED: ~A~%" c)))
//! => cleaning
//!    ESCAPED: ORIGINAL
//! ```
//!
//! `ESCAPED: CLEANUP` is the whole rule. The caller is told about a failure to
//! close a file and never hears about the failure that caused the close.
//!
//! # Why only `error`
//!
//! The cleanup section is walked for a direct call to **`error`** and nothing
//! else. That is a deliberate and narrow choice, because the point of this rule
//! is a call that *cannot* return:
//!
//! - **`signal`** is excluded. An unhandled `signal` returns `nil` and the
//!   unwind carries on, so it destroys nothing by itself.
//! - **`cerror`** is excluded. It establishes a `continue` restart, so it may
//!   well return and let the original condition proceed.
//! - **`assert`** and **`check-type`** are excluded for the same reason: both
//!   are continuable, and both are deliberate invariant checks whose author has
//!   already thought about them.
//! - **`warn`** is excluded: it returns `nil` and transfers nothing.
//!
//! What is left is `error`, which never returns, so a cleanup containing a
//! reachable one always replaces whatever was in flight.
//!
//! # What stops it firing
//!
//! The walk over the cleanup section stops at anything that would *catch* the
//! error before it escapes — `handler-case`, `handler-bind`, `ignore-errors` —
//! because a cleanup that guards its own failure is the correct spelling and
//! must not be reported. It also stops at `lambda`, `defun`, `defmethod` and
//! `defmacro`: an `error` inside a function *defined* in the cleanup is not one
//! the cleanup *calls*.
//!
//! Only the cleanup section is examined. An `error` in the protected form is the
//! ordinary case `unwind-protect` exists for.
//!
//! # The repair, and a rule it must not trip
//!
//! The recommendation is deliberately **not** `ignore-errors`:
//! `paredit-feature-lint-safety`'s `handler-case-swallows-error` reports every
//! `ignore-errors` form whole, so recommending one would be recommending a shape
//! another rule in the same suite reports. The recommendation is a
//! `handler-case` whose body does something — logging the secondary failure —
//! which that rule's `body_is_inert` check leaves alone.
//!
//! Report-only: whether the cleanup's own failure should be logged, ignored, or
//! allowed to win is a decision about this program.
//!
//! Scope: Common Lisp only.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ByteSpan, ExpressionView};
use paredit_core_syntax::view_query::{calls, list_head, symbol_is};

use crate::support::for_each_evaluated_subview_where;

pub const META: RuleMeta = RuleMeta::new(
    "unwind-protect-cleanup-signals",
    RuleCategory::Conditions,
    Severity::Warning,
    "an unwind-protect cleanup form that calls error, replacing whatever condition was unwinding",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Cleanup forms run during the unwind, so an `error` signalled there replaces the condition \
         that caused the unwind. The original is not chained or logged — the caller is told about \
         the cleanup's failure and never hears about the one that started it.",
    )
    .with_example(
        "(unwind-protect (process stream) (unless (finished-p) (error \"incomplete\")))",
        "(unwind-protect (process stream)\n  (handler-case (unless (finished-p) (error \
         \"incomplete\"))\n    (error (c) (log-secondary-failure c))))",
    )
    .with_caveat(
        "Only a direct `error` call is reported, and only one the cleanup would actually reach: \
         the walk stops at `handler-case`, `handler-bind` and `ignore-errors`, and at any function \
         *defined* in the cleanup. `signal`, `cerror`, `assert` and `check-type` are all \
         continuable and are never reported.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("unwind-protect")];

/// Forms that catch an error before it can leave the cleanup, so a call inside
/// one is guarded rather than dangerous.
const CATCHING_FORMS: [&str; 3] = ["handler-case", "handler-bind", "ignore-errors"];

/// Forms that *define* a function rather than call one. An `error` in the body
/// of a `lambda` written in a cleanup is not an error the cleanup signals.
const DEFINING_FORMS: [&str; 4] = ["lambda", "defun", "defmethod", "defmacro"];

/// One cleanup form that can destroy the condition in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignallingCleanup {
    /// The span of the `(error …)` call, which is the thing to guard — not the
    /// whole `unwind-protect`, most of which is fine.
    pub span: ByteSpan,
}

/// Every reachable `error` call in the cleanup section of one `unwind-protect`.
///
/// Local to the node the dispatcher handed over: it walks the cleanup forms and
/// never the protected form, and never the tree.
#[must_use]
pub fn examine(view: &ExpressionView) -> Vec<SignallingCleanup> {
    // `list_head` already implies a paren list, so no separate check: it is
    // defined as `is_paren_list(view).then(|| atom_child(view, 0)).flatten()`.
    if !list_head(view).is_some_and(|head| symbol_is(head, "unwind-protect")) {
        return Vec::new();
    }
    let mut found = Vec::new();
    // `(unwind-protect protected cleanup*)`: the cleanups start at index 2.
    // Index 1 is the protected form, where an `error` is the ordinary case.
    for cleanup in view.children.iter().skip(2) {
        for_each_evaluated_subview_where(
            cleanup,
            |inner| !calls(inner, &CATCHING_FORMS) && !calls(inner, &DEFINING_FORMS),
            |inner| {
                if calls(inner, &["error"]) {
                    found.push(SignallingCleanup { span: inner.span });
                }
            },
        );
    }
    found
}

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let found = examine(view);
        if found.is_empty() {
            return Ok(());
        }
        // `is_unevaluated_at` reaches `root_view()`, so it runs only once there
        // is a finding to suppress.
        if crate::support::is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in found {
            sink.report(
                item.span,
                "an error signalled in a cleanup form replaces the condition that was unwinding, \
                 so the original failure is lost; handle the cleanup's own failure instead"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paredit_core_syntax::dialect::Dialect;
    use paredit_core_syntax::sexpr::SyntaxTree;

    fn count(input: &str) -> usize {
        let tree = SyntaxTree::parse_with_dialect(input, Dialect::CommonLisp).expect("parse");
        let mut total = 0;
        crate::support::for_each_evaluated_subview(&tree.root_view(), |view| {
            total += examine(view).len();
        });
        total
    }

    #[test]
    fn flags_a_direct_error_in_a_cleanup() {
        assert_eq!(
            count("(unwind-protect (work) (error \"cleanup failed\"))"),
            1
        );
    }

    #[test]
    fn flags_an_error_nested_inside_a_cleanup_form() {
        assert_eq!(
            count("(unwind-protect (work) (unless (finished-p) (error \"incomplete\")))"),
            1
        );
    }

    #[test]
    fn flags_each_of_several_cleanup_forms() {
        assert_eq!(
            count("(unwind-protect (work) (error \"a\") (close s) (error \"b\"))"),
            2
        );
    }

    /// The protected form is where an `error` belongs.
    #[test]
    fn does_not_flag_an_error_in_the_protected_form() {
        assert_eq!(count("(unwind-protect (error \"boom\") (close s))"), 0);
    }

    #[test]
    fn does_not_flag_an_ordinary_cleanup() {
        assert_eq!(count("(unwind-protect (work) (close s))"), 0);
        assert_eq!(
            count("(unwind-protect (work) (close s) (delete-file path))"),
            0
        );
    }

    /// The recommended repair must not be reported by the rule that recommends
    /// it.
    #[test]
    fn does_not_flag_a_cleanup_that_handles_its_own_failure() {
        assert_eq!(
            count(
                "(unwind-protect (work)\n  (handler-case (error \"incomplete\")\n    (error (c) \
                 (log-it c))))"
            ),
            0
        );
        assert_eq!(
            count("(unwind-protect (work) (ignore-errors (error \"incomplete\")))"),
            0
        );
        assert_eq!(
            count(
                "(unwind-protect (work) (handler-bind ((error #'log-it)) (error \"incomplete\")))"
            ),
            0
        );
    }

    /// An `error` in a function *defined* in the cleanup is not one the cleanup
    /// signals.
    #[test]
    fn does_not_flag_an_error_inside_a_function_defined_in_the_cleanup() {
        assert_eq!(
            count("(unwind-protect (work) (register (lambda () (error \"later\"))))"),
            0
        );
    }

    /// The continuable signallers are all excluded on purpose.
    #[test]
    fn does_not_flag_the_continuable_signalling_operators() {
        assert_eq!(count("(unwind-protect (work) (signal 'note))"), 0);
        assert_eq!(
            count("(unwind-protect (work) (cerror \"Go on.\" \"x\"))"),
            0
        );
        assert_eq!(count("(unwind-protect (work) (assert (finished-p)))"), 0);
        assert_eq!(count("(unwind-protect (work) (check-type s stream))"), 0);
        assert_eq!(count("(unwind-protect (work) (warn \"odd\"))"), 0);
    }

    #[test]
    fn does_not_flag_an_unwind_protect_with_no_cleanup() {
        // `unwind-protect-no-cleanup` in `paredit-feature-lint-control-flow`
        // owns that shape.
        assert_eq!(count("(unwind-protect (work))"), 0);
    }

    #[test]
    fn a_matching_form_inside_a_quote_is_data() {
        assert_eq!(count("'(unwind-protect (work) (error \"x\"))"), 0);
        assert_eq!(count("`(unwind-protect (work) (error \"x\"))"), 0);
        assert_eq!(count("(quote (unwind-protect (work) (error \"x\")))"), 0);
    }

    #[test]
    fn an_unquoted_form_inside_a_backquote_is_still_code() {
        assert_eq!(count("`(progn ,(unwind-protect (work) (error \"x\")))"), 1);
    }

    /// A macro template that builds an `unwind-protect` is data, and the comma
    /// only re-enters code where it actually appears.
    #[test]
    fn a_cleanup_inside_a_macro_template_is_data() {
        assert_eq!(
            count(
                "(defmacro with-thing (&body body)\n  \
                 `(unwind-protect (progn ,@body) (error \"x\")))"
            ),
            0
        );
    }

    #[test]
    fn a_matching_shape_inside_a_string_literal_is_not_a_form() {
        assert_eq!(
            count("(format t \"(unwind-protect (work) (error \\\"x\\\"))\")"),
            0
        );
    }

    #[test]
    fn the_finding_points_at_the_error_call_not_the_whole_form() {
        let source = "(unwind-protect (work) (error \"cleanup failed\"))";
        let tree = SyntaxTree::parse_with_dialect(source, Dialect::CommonLisp).expect("parse");
        let form = &tree.root_view().children[0];
        let found = examine(form);
        assert_eq!(found.len(), 1);
        assert_eq!(
            &source[found[0].span.start().get()..found[0].span.end().get()],
            "(error \"cleanup failed\")"
        );
    }
}
