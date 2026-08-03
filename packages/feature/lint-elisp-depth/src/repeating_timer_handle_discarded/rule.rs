//! `elisp-repeating-timer-handle-discarded`: a repeating timer nothing can
//! cancel.
//!
//! `run-with-timer`, `run-at-time` and `run-with-idle-timer` all take
//! `(SECS REPEAT FUNCTION &rest ARGS)` and return the timer object, which is
//! the only handle `cancel-timer` accepts. Measured in GNU Emacs 31.0.91:
//!
//! ```text
//! (run-with-timer 3600 3600 #'ignore)   ; value discarded
//! ;; timer-list grew 0 -> 1, and the handle is lost to the program
//! ```
//!
//! A one-shot timer — `REPEAT` nil — is self-limiting and is **not** reported:
//! it fires once and retires, so discarding the handle costs nothing. A
//! repeating timer whose handle is discarded runs until Emacs exits, and the
//! only way back to it is scanning `timer-list` for a function the caller
//! recognises.
//!
//! The whole rule turns on "discarded", which is why it is decidable at all.
//! The value flows somewhere useful when the call is the last child of its
//! parent — the value of an implicit `progn`, the value bound by a `let` pair
//! `(tm (run-with-timer …))`, the return value of the enclosing function — or
//! when the parent form consumes it. Everything else drops it on the floor.
//!
//! # Cost, and why this rule is shaped the way it is
//!
//! Deciding "discarded" needs the node's **parent**, and no public API reaches
//! it: `SyntaxTree::node` is `pub(in crate::sexpr)`, so the only route is
//! `root_view()`, which materializes the whole file as owned views. Measured
//! against `elisp-save-excursion-set-buffer` in the same process:
//!
//! ```text
//! realistic file (no positive literal REPEAT anywhere, zero findings)
//!   this rule        23.7 ns/call (27866 B) -> 23.9 ns/call (55716 B)   flat
//!   shipped baseline 21.1 ns/call           -> 20.5 ns/call
//!
//! pathological file (a repeating timer in every one of 50/100 units)
//!   this rule        84041 ns/call          -> 185475 ns/call   doubling 2.21
//! ```
//!
//! The flat row is the one the `clean/forms/*` gate models, and there this
//! rule costs about 1.15x a shipped rule. The linear row is what a
//! positive-literal REPEAT buys, and it is linear precisely because
//! `root_view()` is linear in the file.
//!
//! [`repeats`] is therefore load-bearing for cost as well as for correctness:
//! it is the last check that can reject without reaching the root. Across GNU
//! Emacs's `lisp/` tree plus 2585 third-party files there are 500 timer call
//! sites and only **33** carry a positive literal REPEAT, so the expensive
//! path is taken for 6.6% of an already-rare head. Moving `repeats` after the
//! walk would make every one of the 500 pay it.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::{ancestry_at, atom_text, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-repeating-timer-handle-discarded",
    RuleCategory::Resource,
    Severity::Warning,
    "a repeating timer whose returned handle is discarded, leaving nothing to cancel it with",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`run-with-timer`, `run-at-time` and `run-with-idle-timer` return the timer object, and \
         `cancel-timer` takes nothing else. When the REPEAT argument is non-nil the timer fires \
         forever, so discarding the return value leaves the program with no way to stop it — the \
         only route back is scanning `timer-list`. A one-shot timer is self-limiting and is not \
         reported.",
    )
    .with_example(
        "(run-with-timer 0 60 #'my-refresh)",
        "(defvar my-refresh-timer nil)\n(setq my-refresh-timer (run-with-timer 0 60 #'my-refresh))",
    )
    .with_caveat(
        "Only a literally non-nil REPEAT argument counts. `(run-with-timer 5 nil #'f)` is a \
         one-shot and is left alone, and a REPEAT the rule cannot read — a variable, a call — is \
         also left alone rather than guessed at.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("run-with-timer"),
    NormalizedHead::new("run-at-time"),
    NormalizedHead::new("run-with-idle-timer"),
];

/// Forms that consume the value of an argument that is not their last child.
///
/// Being an argument to any of these means the timer object reaches something
/// that can hold on to it.
const CONSUMING_FORMS: [&str; 8] = [
    "setq",
    "setq-local",
    "setq-default",
    "set",
    "push",
    "add-to-list",
    "puthash",
    "process-put",
];

/// Whether the REPEAT argument is a literal **positive number**.
///
/// "Non-nil" is the wrong test, and the corpus caught it. Measured in GNU
/// Emacs 31.0.91 by reading `timer--repeat-delay` off the returned timer:
///
/// ```text
/// REPEAT=0     -> repeat-delay=nil     REPEAT=0.5 -> repeat-delay=0.5
/// REPEAT=0.0   -> repeat-delay=nil     REPEAT=1   -> repeat-delay=1
/// REPEAT=-1    -> repeat-delay=nil     REPEAT=60  -> repeat-delay=60
/// REPEAT=t     -> repeat-delay=nil
/// ```
///
/// So `0` and `t` are both one-shots. An earlier version of this treated
/// either as repeating and reported `pulse.el:260`, which passes `0`
/// deliberately and is correct.
///
/// Only a literal settles this at all. A variable or a call could be either,
/// and reporting on one would be a false positive on every caller that passes
/// nil.
fn repeats(view: &ExpressionView) -> bool {
    let Some(repeat) = view.children.get(2) else {
        return false;
    };
    // A list — `(if x 60 nil)`, `(my-interval)` — is not a literal.
    let Some(text) = atom_text(repeat) else {
        return false;
    };
    text.parse::<f64>().is_ok_and(|seconds| seconds > 0.0)
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
        // The cheap domain checks, before anything reaches the root: the head,
        // then the REPEAT argument, both read off the node already in hand.
        //
        // The head comparison kills no mutation, because `HeadFilter::Heads`
        // has already filtered on exactly these three names. It stays for the
        // reason the sibling `lint-elisp-idiom` rules keep theirs: the head
        // index documents itself as a pre-filter that "must never be narrower
        // than any rule's notion of the same operator, but may be wider", so a
        // rule that leaned on it would be correct only by accident. It is also
        // what keeps `root_view()` behind a comparison rather than in front of
        // one.
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        if !matches!(
            head,
            "run-with-timer" | "run-at-time" | "run-with-idle-timer"
        ) {
            return Ok(());
        }
        if !repeats(view) {
            return Ok(());
        }
        let root = context.tree().root_view();
        let ancestry = ancestry_at(&root, view.span);
        if ancestry.is_data {
            return Ok(());
        }
        if ancestry.value_is_used(&CONSUMING_FORMS) {
            return Ok(());
        }
        sink.report(
            view.span,
            format!(
                "this `{head}` repeats, and its returned handle is discarded; \
                 nothing can pass it to cancel-timer"
            ),
        );
        Ok(())
    }
}
