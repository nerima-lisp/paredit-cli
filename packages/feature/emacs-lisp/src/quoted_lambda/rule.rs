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
//!
//! Which is exactly why the head alone is not enough to fire on. A quoted list
//! whose first element is the symbol `lambda` is not necessarily a lambda
//! expression, and GNU Emacs contains several that are not: `bind-key.el`,
//! `cus-start.el`, `calc-map.el`, `elint.el` and `byte-opt.el` all write
//! `(memq … '(lambda …))` against a list of *symbol names*. Dropping the quote
//! there rewrites a membership test into a call, so two things are checked
//! beyond the head — that the form has a lambda list, and that the quote is
//! the only reader prefix on it.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleFix, RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use crate::shared::atom_text;

/// Whether `view` is the shape a lambda expression actually has.
///
/// A lambda expression's second element is its lambda list: a `(…)` list, or
/// the atom `nil` for the argument-less spelling `(lambda nil …)`. A quoted
/// list that merely *starts* with the symbol `lambda` need not have one, and
/// in practice usually does not — `'(lambda function)` and
/// `'(lambda calcFunc-lambda)` are symbol lists a `memq` searches, and
/// `'(lambda)` is a one-element list of the symbol. GNU Emacs writes all
/// three, and dropping the quote from any of them turns a membership test
/// into a call.
///
/// So the lambda list is the discriminator, and it is a cheap one: the second
/// child of a node the caller has already matched.
fn has_a_lambda_list(view: &ExpressionView) -> bool {
    match view.children.get(1) {
        Some(second) => second.kind == ExpressionKind::List || atom_text(second) == Some("nil"),
        None => false,
    }
}

/// Whether the only reader prefix on the form is the quote this rule is about.
///
/// `reader_prefixes` is in source order, so `',(lambda …)` is
/// `[Quote, Unquote]` and `''(lambda …)` is `[Quote, Quote]`. Only a lone
/// `Quote` means what the rule claims:
///
/// - **`',(lambda …)`** — inside a backquote the unquote *evaluates* the
///   lambda, so the quote applies to the resulting closure. Embedding a
///   closure in a generated form this way is how `menu-bar.el` writes a
///   `menu-item` `:enable` expression.
/// - **`''(lambda …)`** and **`'#'(lambda …)`** — a second quote makes the
///   whole thing data by construction; nothing here is going to be called.
fn is_plainly_quoted(view: &ExpressionView) -> bool {
    view.reader_prefixes.as_slice() == [ReaderPrefix::Quote]
}

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
        "The head must be the symbol `lambda`, the form must have a lambda list, and the quote \
         must be the only reader prefix. So `'(lambda function)` — a symbol list a `memq` \
         searches — and `',(lambda () …)` — a closure spliced into a backquote — are both left \
         alone.",
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
        if view.kind != ExpressionKind::List || !is_plainly_quoted(view) {
            return Ok(());
        }
        if view.children.first().and_then(atom_text) != Some("lambda") {
            return Ok(());
        }
        if !has_a_lambda_list(view) {
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
