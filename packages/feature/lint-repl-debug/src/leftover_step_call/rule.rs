//! `leftover-step-call`: a Common Lisp (step form) wrapper left in committed source.
//!
//! The analysis lives in [`crate::leftover_step_call::domain`], which also backs the
//! standalone `inspect leftover-step-call` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::leftover_step_call::domain::examine;
use crate::support::OperatorScope;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "leftover-step-call",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Common Lisp (step form) wrapper left in committed source",
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
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        let scope = OperatorScope::shared(context);
        examine(
            view,
            &scope,
            context.path(),
            &mut scanned_form_count,
            &mut items,
        );
        for item in items {
            let message = "step is a leftover stepping wrapper".to_owned();
            // `step` returns exactly `form`'s own value(s) (CLHS), so
            // unwrapping is safe in every *position*. `form_span` is still
            // `None` when the head resolves to a local function of the same
            // name.
            match item.form_span {
                Some(form_span) => {
                    let fix = RuleFix::single(
                        item.span,
                        context.slice(form_span).to_owned(),
                        "Unwrap the `step` wrapper".to_owned(),
                    );
                    sink.report_fixed(item.span, message, fix);
                }
                None => sink.report(item.span, message),
            }
        }
        Ok(())
    }
}
