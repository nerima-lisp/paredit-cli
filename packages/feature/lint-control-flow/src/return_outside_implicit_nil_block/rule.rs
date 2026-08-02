//! `return-outside-implicit-nil-block`: a `(return …)` no enclosing form
//! establishes the implicit `nil` block for.
//!
//! The analysis lives in [`crate::return_outside_implicit_nil_block::domain`],
//! which also backs the standalone `inspect
//! return-outside-implicit-nil-block` command; this module only registers it
//! with the lint suite and phrases its findings.
//!
//! `ReportOnly`: the repair is to wrap the intended form in a block or to
//! rewrite the exit, and either changes what the program returns.
//!
//! # Cost
//!
//! `Heads(["return"])`, so a file with no `return` pays one hash lookup per
//! list node — what the `clean/forms/*` benchmarks measure. The ancestor walk
//! materializes only the enclosing top-level form.

use paredit_core_lint_engine::LintResult;

use crate::return_outside_implicit_nil_block::domain::{MESSAGE, examine_return};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "return-outside-implicit-nil-block",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a return with no enclosing form establishing the implicit nil block",
    Fixability::ReportOnly,
);

/// `examine_return` only ever matches a `return` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("return")];

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
        let mut return_form_count = 0;
        let mut items = Vec::new();
        examine_return(context.tree(), view, &mut return_form_count, &mut items);
        for item in items {
            sink.report(item.span, MESSAGE.to_owned());
        }
        Ok(())
    }
}
