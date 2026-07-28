//! `elisp-quoted-lambda`: `'(lambda …)` where a function was meant.
//!
//! `(quote (lambda …))` yields a *list* whose first element is the symbol
//! `lambda`. Under dynamic binding that list is still funcallable, which is
//! why the idiom survived; under `lexical-binding: t` it is not a closure, so
//! it captures nothing and any variable its body reads from the enclosing
//! scope is unbound at call time.
//!
//! The byte compiler also cannot compile a quoted lambda, so one inside a hot
//! path stays interpreted.
//!
//! Fixable, and the fix is the smallest edit that is also a repair: dropping
//! the quote. A bare `(lambda …)` under `lexical-binding: t` evaluates to a
//! closure, which is what the code meant; `#'(lambda …)` is sugar for exactly
//! that and one more character to write.
//!
//! The rewrite is tagged `destructive` because it is the rare fix that changes
//! what the form *evaluates to* — a closure instead of a list — rather than
//! only how it is spelled. That is the point of the rule, and a reader
//! auditing an automated run should see the distinction.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleFix, RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use crate::shared::atom_text;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-quoted-lambda",
    RuleCategory::Suspicious,
    Severity::Error,
    "a quoted lambda, which is a list rather than a closure under lexical binding",
    Fixability::Fixable,
)
.with_tags(&[RuleTag::Destructive])
.with_explanation(
    RuleExplanation::new(
        "`(quote (lambda …))` is a list whose first element is the symbol `lambda`. Under \
         `lexical-binding: t` it captures nothing, so any variable its body reads from the \
         enclosing scope is unbound when it runs — and the byte compiler cannot compile it.",
    )
    .with_example(
        "(mapcar '(lambda (x) (+ x n)) xs)",
        "(mapcar (lambda (x) (+ x n)) xs)",
    )
    .with_caveat(
        "A quoted list that merely starts with something else is not reported: the head must be \
         the symbol `lambda`.",
    ),
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // Keyed on the reader prefix rather than the head, so the filter has
        // to see every node.
        HeadFilter::AllNodes
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
        if view.kind != ExpressionKind::List || !view.reader_prefixes.contains(&ReaderPrefix::Quote)
        {
            return Ok(());
        }
        if view.children.first().and_then(atom_text) != Some("lambda") {
            return Ok(());
        }

        // `content_span` is the form after its reader prefixes, so replacing
        // the whole span with that slice is exactly "drop the quote" — no
        // re-printing, and the lambda's own formatting survives.
        sink.report_fixed(
            view.span,
            "a quoted lambda evaluates to a list, not a closure; drop the \
             quote or write `#'` so the body captures its enclosing scope",
            RuleFix::single(
                view.span,
                context.slice(view.content_span).to_owned(),
                "Drop the quote so the lambda is a closure",
            ),
        );
        Ok(())
    }
}
