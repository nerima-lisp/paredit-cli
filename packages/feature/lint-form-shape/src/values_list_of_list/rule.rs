//! `values-list-of-list`: a values-list of a list constructor ((values-list (list a b)) is (values a b)).
//!
//! The analysis lives in [`crate::values_list_of_list::domain`], which also backs the
//! standalone `inspect values-list-of-list` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::support::is_hard_quoted_at;
use crate::values_list_of_list::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "values-list-of-list",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a values-list of a list constructor ((values-list (list a b)) is (values a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("values-list")];

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
        let mut values_list_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut values_list_form_count, &mut items);
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
                // (values-list (list a b)) is (values a b); an empty list -> (values).
                let text = match item.elements_span {
                    Some(span) => format!("(values {})", context_slice(span)),
                    None => "(values)".to_owned(),
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
                    "Rewrite (values-list (list …)) as (values …)".to_owned(),
                )
            };

            sink.report_fixed(span, "values-list of a fresh list is just values; (values-list (list a b)) is (values a b)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
