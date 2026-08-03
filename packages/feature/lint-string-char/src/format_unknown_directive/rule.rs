//! `format-unknown-directive`: a ~ directive in a literal format control string that CLHS 22.3 does not define.
//!
//! The analysis lives in [`crate::format_unknown_directive::domain`], which also backs the
//! standalone `inspect format-unknown-directive` command; this module only registers it
//! with the lint suite and phrases its findings.
//!
//! `HeadFilter::Heads`, never `AllNodes`: the five `format`-family operators are
//! the only nodes whose control string this rule can read, and the
//! `clean/forms/*` benchmarks measure exactly what a rule costs a file it says
//! nothing about. Everything expensive — resolving the literal's escapes,
//! scanning its directives, and asking whether the call is quoted data — happens
//! behind the cheap disqualifiers in [`crate::support::literal_control_string`].

use paredit_core_lint_engine::LintResult;

use crate::format_unknown_directive::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "format-unknown-directive",
    RuleCategory::Malformed,
    Severity::Warning,
    "a ~ directive in a literal format control string that CLHS 22.3 does not define",
    Fixability::ReportOnly,
);

/// The `format`-family operators whose control string is a fixed argument.
/// Matches `crate::support`'s own table, which decides *which* argument.
const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("format"),
    NormalizedHead::new("error"),
    NormalizedHead::new("warn"),
    NormalizedHead::new("cerror"),
    NormalizedHead::new("format-to-string"),
];

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
        let mut control_string_count = 0;
        let mut items = Vec::new();
        examine(view, &mut control_string_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }

        // Only with a violation in hand is it worth asking whether this call is
        // code at all. The dispatcher hands a rule every head-matched node,
        // including the ones inside `'(...)`, and `is_unevaluated_at`
        // materializes the document to answer — so a file with no findings
        // never reaches it.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }

        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
