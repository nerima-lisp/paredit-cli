//! `leftover-print-debug`: a bare debug-print call left in committed source.
//!
//! The analysis lives in [`crate::leftover_print_debug::domain`], which also backs the
//! standalone `inspect leftover-print-debug` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_print_debug::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "leftover-print-debug",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a bare debug-print call left in committed source",
    Fixability::Fixable,
);

/// Every dialect [`crate::leftover_print_debug::domain::heads_for`] models a
/// debug-print vocabulary for.
const DIALECTS: [Dialect; 8] = [
    Dialect::CommonLisp,
    Dialect::EmacsLisp,
    Dialect::Clojure,
    Dialect::Scheme,
    Dialect::Racket,
    Dialect::Janet,
    Dialect::Fennel,
    Dialect::Hy,
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        // The removal-safety analysis needs to see a call's position relative
        // to its enclosing body and any surrounding quote, which is not a
        // per-node predicate — see `paredit-feature-lint-repl-debug::support`.
        HeadFilter::WholeTree
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
        let candidates = evaluated_candidates(context, view);
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context);
        examine(
            candidates,
            &scope,
            context.dialect(),
            context.path(),
            &mut items,
        );
        for item in items {
            let message = format!("{} is a leftover debug-print call", item.head);
            match item.fix_span {
                Some(fix_span) => {
                    let fix = RuleFix::single(
                        fix_span,
                        String::new(),
                        format!("Remove the leftover {} call", item.head),
                    );
                    sink.report_fixed(item.span, message, fix);
                }
                None => sink.report(item.span, message),
            }
        }
        Ok(())
    }
}
