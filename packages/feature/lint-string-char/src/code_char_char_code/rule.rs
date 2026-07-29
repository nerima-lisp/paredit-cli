//! `code-char-char-code`: a code-char of a char-code, a round-trip that is just the character ((code-char (char-code c)) is c).
//!
//! The analysis lives in [`crate::code_char_char_code::domain`], which also backs the
//! standalone `inspect code-char-char-code` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::code_char_char_code::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "code-char-char-code",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a code-char of a char-code, a round-trip that is just the character ((code-char (char-code c)) is c)",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("code-char")];

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
        let mut code_char_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.source(),
            &mut code_char_form_count,
            &mut items,
        );
        for item in items {
            let span = item.span;
            let fix = {
                // (code-char (char-code c)) is c.

                RuleFix::single(
                    item.span,
                    context_slice(item.char_span),
                    "Drop the code-char/char-code round-trip".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "code-char of char-code is a round-trip; (code-char (char-code c)) is c".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
