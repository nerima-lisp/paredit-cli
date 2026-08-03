//! `scheme-let-star-independent-bindings`: a `let*` whose bindings cannot
//! depend on one another.
//!
//! The analysis lives in [`crate::let_star_independent_bindings::domain`],
//! which also backs the standalone
//! `inspect scheme-let-star-independent-bindings` command; this module only
//! registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::let_star_independent_bindings::domain::{DIALECTS, HEAD, examine_let_star, message_for};

pub const META: RuleMeta = RuleMeta::new(
    "scheme-let-star-independent-bindings",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let* whose initializers are all literals or free references, so no binding depends on another",
    Fixability::Fixable,
);

/// `examine_let_star` only ever matches a `let*` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new(HEAD)];

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
    ) -> LintResult<()> {
        let mut let_star_form_count = 0;
        let mut items = Vec::new();
        examine_let_star(context.tree(), view, &mut let_star_form_count, &mut items);
        for item in items {
            // Only the head symbol is rewritten, so the binding list, body,
            // spacing and comments stay byte-identical.
            let fix = RuleFix::single(
                item.head_span,
                "let".to_owned(),
                "Rewrite the let* as a let".to_owned(),
            );
            sink.report_fixed(item.span, message_for(item.binding_count), fix);
        }
        Ok(())
    }
}
