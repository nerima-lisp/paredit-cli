//! `nested-unless`: an unless whose only body is an unless, mergeable by or ((unless a (unless b c)) is (unless (or a b) c)).
//!
//! The analysis lives in [`crate::nested_unless::domain`], which also backs the
//! standalone `inspect nested-unless` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::nested_unless::domain::examine_unless;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "nested-unless",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an unless whose only body is an unless, mergeable by or ((unless a (unless b c)) is (unless (or a b) c))",
    Fixability::Fixable,
);

/// `examine_unless` only ever matches an `unless` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("unless")];

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
        let mut unless_form_count = 0;
        let mut items = Vec::new();
        examine_unless(view, context.path(), &mut unless_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // Merge the two tests: (unless a (unless b body)) -> (unless (or a b) body).
                let or = format!(
                    "(or {} {})",
                    context_slice(item.outer_test_span),
                    context_slice(item.inner_test_span)
                );
                let text = match item.inner_body_span {
                    Some(body) => format!("(unless {} {})", or, context_slice(body)),
                    None => format!("(unless {or})"),
                };

                RuleFix::single(
                    item.span,
                    text,
                    "Merge the nested unless tests with or".to_owned(),
                )
            };

            sink.report_fixed(span, "unless whose only body is an unless merges by or; (unless a (unless b c)) is (unless (or a b) c)"
                            .to_owned(), fix);
        }
        Ok(())
    }
}
