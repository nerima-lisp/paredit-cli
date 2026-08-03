//! `scheme-named-let-never-recurs`: a named `let` whose loop name is never
//! mentioned in its body.
//!
//! The analysis lives in [`crate::named_let_never_recurs::domain`], which also
//! backs the standalone `inspect scheme-named-let-never-recurs` command; this
//! module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::named_let_never_recurs::domain::{DIALECTS, HEAD, examine_named_let, message_for};

pub const META: RuleMeta = RuleMeta::new(
    "scheme-named-let-never-recurs",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a named let whose loop name is never mentioned in its body, so it can never iterate",
    Fixability::Fixable,
);

/// `examine_named_let` only ever matches a `let` head.
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
        let mut named_let_form_count = 0;
        let mut items = Vec::new();
        examine_named_let(context.tree(), view, &mut named_let_form_count, &mut items);
        for item in items {
            let message = message_for(&item.name);
            match item.fix_span {
                Some(fix_span) => {
                    let fix = RuleFix::single(
                        fix_span,
                        String::new(),
                        format!("Remove the unused loop name {}", item.name),
                    );
                    sink.report_fixed(item.span, message, fix);
                }
                None => sink.report(item.span, message),
            }
        }
        Ok(())
    }
}
