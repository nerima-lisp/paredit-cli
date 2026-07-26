//! `redundant-funcall`: a funcall of a sharp-quoted symbol ((funcall #'foo a b) is just (foo a b)).
//!
//! The analysis lives in [`crate::domain::redundant_funcall_report`], which also backs the
//! standalone `inspect redundant-funcall` command; this module only registers it with
//! the lint suite and phrases its findings.

use anyhow::Result;

use crate::domain::lint::engine::{RuleContext, RuleSink};
use crate::domain::lint::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use crate::domain::lint::rule::LintRule;
use crate::domain::redundant_funcall_report::examine_funcall;
use crate::domain::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-funcall",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a funcall of a sharp-quoted symbol ((funcall #'foo a b) is just (foo a b))",
    Fixability::Fixable,
);

/// `examine_funcall` only ever matches a `funcall` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("funcall")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> Result<()> {
        let mut funcall_form_count = 0;
        let mut items = Vec::new();
        examine_funcall(view, context.path(), &mut funcall_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                let item = item.clone();
                // Delete `funcall ` and the `#'` prefix in one cut, from the funcall
                // head up to the callee symbol, leaving `(foo …)` byte-identical.

                RuleFix::multi(
                    format!("Rewrite (funcall #'{} …) as a direct call", item.callee),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "funcall of #'{} is a direct call; (funcall #'{} …) is ({} …)",
                    item.callee, item.callee, item.callee
                ),
                fix,
            );
        }
        Ok(())
    }
}
