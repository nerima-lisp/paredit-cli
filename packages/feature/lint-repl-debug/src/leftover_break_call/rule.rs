//! `leftover-break-call`: a Common Lisp (break ...) left in committed source.
//!
//! The analysis lives in [`crate::leftover_break_call::domain`], which also backs the
//! standalone `inspect leftover-break-call` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_break_call::domain::examine;
use crate::support::{OperatorScope, evaluated_candidates};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "leftover-break-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Common Lisp (break ...) left in committed source",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::WholeTree
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
        examine(candidates, &scope, context.path(), &mut items);
        for item in items {
            let message = "break is a leftover interactive-debugger entry point".to_owned();
            match item.fix_span {
                Some(fix_span) => {
                    let fix = RuleFix::single(
                        fix_span,
                        String::new(),
                        "Remove the leftover break call".to_owned(),
                    );
                    sink.report_fixed(item.span, message, fix);
                }
                None => sink.report(item.span, message),
            }
        }
        Ok(())
    }
}
