//! `leftover-trace-call`: `trace`/`untrace` used as a statement, left in committed source.
//!
//! The analysis lives in [`crate::leftover_trace_call::domain`], which also backs the
//! standalone `inspect leftover-trace-call` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_trace_call::domain::examine;
use crate::support::OperatorScope;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "leftover-trace-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "trace or untrace used as a statement, left in committed source",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&[Dialect::CommonLisp, Dialect::EmacsLisp])
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context.binding_table());
        examine(
            view,
            &scope,
            context.path(),
            &mut scanned_form_count,
            &mut items,
        );
        for item in items {
            let message = format!("{} is a leftover debugging statement", item.head);
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
