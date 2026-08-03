//! `elisp-hook-lambda`: a lambda passed to `add-hook` or `remove-hook`.
//!
//! `add-hook`'s own docstring says it, in GNU Emacs 31.0.91:
//!
//! > FUNCTION may be any valid function, but it's recommended to use a
//! > function symbol and not a lambda form. Using a symbol will ensure that
//! > the function is not re-added if the function is edited, and using lambda
//! > forms may also have a negative performance impact when running `add-hook`
//! > and `remove-hook`.
//!
//! The reason usually given for this rule — that `remove-hook` can never
//! remove a lambda — is **not** what happens, and this rule does not claim it.
//! Measured: two textually identical `(lambda () 1)` forms added to the same
//! hook leave it at length 1, and `(remove-hook 'h (lambda () 1))` empties it,
//! because both comparisons are `equal` and a closure over identical captures
//! is `equal` to its twin. What actually bites is the docstring's first
//! reason: *edit* the lambda and re-evaluate the file, and the old one is
//! still on the hook while the new one is added beside it.
//!
//! A quote-prefixed lambda is left to `elisp-quoted-lambda`, whose complaint
//! — that `'(lambda …)` is a list and captures nothing — is strictly stronger
//! than this one, and two findings on one form is noise.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::shared::{is_lambda_form, is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "elisp-hook-lambda",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a lambda on a hook, which re-evaluating the file adds a second copy of",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`add-hook` compares with `equal`, so an anonymous function is deduplicated only while it \
         is textually unchanged. Edit the lambda and re-evaluate the file and the hook holds both \
         versions; `add-hook`'s own docstring recommends a function symbol for exactly this \
         reason, and notes the comparison cost as a second one.",
    )
    .with_example(
        "(add-hook 'text-mode-hook (lambda () (setq fill-column 72)))",
        "(defun my-text-setup () (setq fill-column 72))\n\
         (add-hook 'text-mode-hook #'my-text-setup)",
    )
    .with_caveat(
        "`'(lambda …)` is not reported here: `elisp-quoted-lambda` already reports it, and says \
         more.",
    ),
);

/// `(add-hook HOOK FUNCTION …)` and `(remove-hook HOOK FUNCTION …)` agree on
/// where FUNCTION sits.
const FUNCTION_INDEX: usize = 2;

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("add-hook"),
    NormalizedHead::new("remove-hook"),
];

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
        // Load-bearing beyond the guard: the two heads earn different advice,
        // so the message below needs to know which one this is.
        let head = match list_head(view) {
            Some(head @ ("add-hook" | "remove-hook")) => head,
            _ => return Ok(()),
        };
        let Some(function) = view.children.get(FUNCTION_INDEX) else {
            return Ok(());
        };
        if !is_lambda_form(function) {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        let advice = if head == "add-hook" {
            "re-evaluating this file after editing the lambda leaves the old \
             one on the hook and adds the new one beside it"
        } else {
            "this lambda is `equal` to the one on the hook only while both \
             are spelled identically, so the removal silently does nothing \
             once either is edited"
        };
        sink.report(
            function.span,
            format!("{head} is given a lambda rather than a function symbol; {advice}"),
        );
        Ok(())
    }
}
