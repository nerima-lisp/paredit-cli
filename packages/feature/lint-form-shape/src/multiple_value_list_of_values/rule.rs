//! `multiple-value-list-of-values`: a multiple-value-list of a values form ((multiple-value-list (values a b)) is (list a b)).
//!
//! The analysis lives in [`crate::multiple_value_list_of_values::domain`], which also backs the
//! standalone `inspect multiple-value-list-of-values` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::multiple_value_list_of_values::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "multiple-value-list-of-values",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multiple-value-list of a values form ((multiple-value-list (values a b)) is (list a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("multiple-value-list")];

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
    ) -> LintResult<()> {
        let context_slice = |span| context.slice(span).to_owned();
        let mut mvl_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut mvl_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (multiple-value-list (values a b)) is (list a b); empty -> (list).
                let text = match item.elements_span {
                    Some(span) => format!("(list {})", context_slice(span)),
                    None => "(list)".to_owned(),
                };

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                RuleFix::single(
                    view.content_span,
                    text,
                    "Rewrite (multiple-value-list (values …)) as (list …)".to_owned(),
                )
            };

            sink.report_fixed(span, "multiple-value-list of a values form is just list; (multiple-value-list (values a b)) is (list a b)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
