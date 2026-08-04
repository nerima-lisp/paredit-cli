//! `nested-cxr`: nested car/cdr accessors that combine into one ((car (cdr x)) is (cadr x)).
//!
//! The analysis lives in [`crate::nested_cxr::domain`], which also backs the
//! standalone `inspect nested-cxr` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nested_cxr::domain::examine_accessor;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-cxr",
    RuleCategory::Suspicious,
    Severity::Warning,
    "nested car/cdr accessors that combine into one ((car (cdr x)) is (cadr x))",
    Fixability::Fixable,
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    /// `examine_accessor` matches heads by the `c[ad]+r` pattern (any of the
    /// 30 standard cXr names), not an enumerable set, so every node must be
    /// offered to it.
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::AllNodes
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut accessor_form_count = 0;
        let mut items = Vec::new();
        examine_accessor(view, &mut accessor_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Collapse to the combined accessor over the innermost argument.
                let text = format!("({} {})", item.combined, context_slice(item.arg_span));

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    format!("Combine the nested accessors into {}", item.combined),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "nested car/cdr accessors combine into ({} …)",
                    item.combined
                ),
                fix,
            );
        }
        Ok(())
    }
}
