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

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{Fixability, HeadFilter, RuleCategory, RuleMeta, Severity};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::{ExpressionKind, ExpressionView, ReaderPrefix};

use crate::shared::atom_text;

pub const META: RuleMeta = RuleMeta::new(
    "elisp-quoted-lambda",
    RuleCategory::Suspicious,
    Severity::Error,
    "a quoted lambda, which is a list rather than a closure under lexical binding",
    Fixability::ReportOnly,
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
        _context: &RuleContext<'_>,
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

        sink.report(
            view.span,
            "a quoted lambda evaluates to a list, not a closure; drop the \
             quote or write `#'` so the body captures its enclosing scope",
        );
        Ok(())
    }
}
