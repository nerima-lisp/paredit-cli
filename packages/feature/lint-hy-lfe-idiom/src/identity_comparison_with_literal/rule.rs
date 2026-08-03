//! `hy-identity-comparison-with-literal`: `(is x 5)` — an object-identity test
//! against a value literal.
//!
//! Hy's `is` is Python's `is`, so this asks whether two names refer to the
//! *same object*, and whether they do for a literal is an interning accident.
//! CPython's own compiler warns about it, and because Hy compiles to a Python
//! AST the warning reaches Hy code unchanged. Measured, Hy 1.3.1 on CPython
//! 3.14.6:
//!
//! ```text
//! $ hy -c '(setv x 5) (print (is x 5))'
//! <string>:1: SyntaxWarning: "is" with 'int' literal. Did you mean "=="?
//! True
//! ```
//!
//! The `True` is the trap: small integers and interned strings compare
//! identical until the value leaves the cached range, at which point the same
//! code silently starts answering `False`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::identity_comparison_with_literal::domain::{DIALECTS, is_value_literal};
use crate::shared::{is_unevaluated_at, list_head};

pub const META: RuleMeta = RuleMeta::new(
    "hy-identity-comparison-with-literal",
    RuleCategory::Suspicious,
    Severity::Warning,
    "`is` compared against a value literal, which tests object identity, not equality",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Hy's `is` compiles to Python's `is`, which asks whether two expressions are the *same \
         object*. For a number or a string that is an interning accident: CPython caches small \
         integers and short strings, so `(is x 5)` answers `True` today and starts answering \
         `False` the moment the value leaves the cached range. CPython's own compiler emits \
         `SyntaxWarning: \"is\" with 'int' literal. Did you mean \"==\"?` for exactly this \
         shape, and the warning reaches Hy unchanged because Hy compiles to a Python AST.",
    )
    .with_example(
        "(when (is status 200)\n  (handle))",
        "(when (= status 200)\n  (handle))",
    )
    .with_caveat(
        "`None`, `True` and `False` are singletons, so `(is x None)` is the *correct* spelling \
         and is never reported — which is also exactly where CPython stays silent.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("is"),
    NormalizedHead::new("is-not"),
    NormalizedHead::new("is_not"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // Load-bearing beyond the guard: the message names the operator, and
        // `is-not`'s replacement is `!=` rather than `=`.
        let Some(head) = list_head(view) else {
            return Ok(());
        };
        // `(is)` and `(is x)` are not comparisons; Hy's `is` is variadic and
        // chains, so every operand past the first is compared.
        if view.children.len() < 3 {
            return Ok(());
        }
        let literals: Vec<&ExpressionView> = view
            .children
            .iter()
            .skip(1)
            .filter(|operand| is_value_literal(operand))
            .collect();
        if literals.is_empty() {
            return Ok(());
        }
        // Only now, with a finding otherwise ready: this reads the root view.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        let replacement = if head == "is" { "=" } else { "!=" };
        for literal in literals {
            sink.report(
                literal.span,
                format!(
                    "`{head}` compares object identity, so this literal matches only by \
                     interning accident; use `{replacement}` to compare values"
                ),
            );
        }
        Ok(())
    }
}
