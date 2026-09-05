//! `go-to-undefined-tag`: a `go` naming a tag no enclosing tagbody
//! establishes.
//!
//!
//! `ReportOnly`: there is no mechanical repair — the tag is either missing or
//! misspelled, and only the author knows which.
//!
//! # Cost
//!
//! `Heads(["go"])`, so a file with no `go` pays one hash lookup per list node.
//! The ancestor walk materializes only the enclosing top-level form.

use paredit_core_lint_engine::LintResult;

use crate::go_to_undefined_tag::domain::{examine_go, message_for};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "go-to-undefined-tag",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a go targeting a tag no enclosing tagbody establishes",
    Fixability::ReportOnly,
);

/// `examine_go` only ever matches a `go` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("go")];

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
        let mut go_form_count = 0;
        let mut items = Vec::new();
        examine_go(context.tree(), view, &mut go_form_count, &mut items);
        for item in items {
            sink.report(item.span, message_for(&item.tag));
        }
        Ok(())
    }
}
