//! `double-reverse`: a (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x)).
//!
//! The analysis lives in [`crate::double_reverse::domain`], which also backs the
//! standalone `inspect double-reverse` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::double_reverse::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "double-reverse",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a (reverse (reverse x)), a wasteful obfuscated copy ((reverse (reverse x)) is (copy-seq x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("reverse")];

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
        let mut reverse_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut reverse_form_count, &mut items);
        for item in items {
            let span = item.span;
            // Rewriting hard-quoted data edits a user's data literal rather than
            // code, and no round-trip property catches it. Read on the `hard`
            // counter alone: a `` `(…) `` template's contents really are emitted as
            // code. See `support::is_hard_quoted_at`.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // (reverse (reverse x)) is (copy-seq x): keep the inner argument.
                let text = format!("(copy-seq {})", context_slice(item.inner_span));

                RuleFix::single(
                    item.span,
                    text,
                    "Rewrite (reverse (reverse x)) as (copy-seq x)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "(reverse (reverse x)) is a wasteful copy; use (copy-seq x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
