//! `redundant-end-nil`: a bounded-sequence call with an explicit :end nil, the default ((find x seq :end nil) is (find x seq)).
//!
//! The analysis lives in [`crate::redundant_end_nil::domain`], which also backs the
//! standalone `inspect redundant-end-nil` command; this module only registers it with
//! the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::redundant_end_nil::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-end-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a bounded-sequence call with an explicit :end nil, the default ((find x seq :end nil) is (find x seq))",
    Fixability::Fixable,
);

/// Single-sequence operators whose `:end` defaults to `nil`; mirrors
/// `END_HEADS` in the report module.
const HEADS: [NormalizedHead; 35] = [
    NormalizedHead::new("find"),
    NormalizedHead::new("find-if"),
    NormalizedHead::new("find-if-not"),
    NormalizedHead::new("position"),
    NormalizedHead::new("position-if"),
    NormalizedHead::new("position-if-not"),
    NormalizedHead::new("count"),
    NormalizedHead::new("count-if"),
    NormalizedHead::new("count-if-not"),
    NormalizedHead::new("remove"),
    NormalizedHead::new("remove-if"),
    NormalizedHead::new("remove-if-not"),
    NormalizedHead::new("delete"),
    NormalizedHead::new("delete-if"),
    NormalizedHead::new("delete-if-not"),
    NormalizedHead::new("substitute"),
    NormalizedHead::new("substitute-if"),
    NormalizedHead::new("substitute-if-not"),
    NormalizedHead::new("nsubstitute"),
    NormalizedHead::new("nsubstitute-if"),
    NormalizedHead::new("nsubstitute-if-not"),
    NormalizedHead::new("remove-duplicates"),
    NormalizedHead::new("delete-duplicates"),
    NormalizedHead::new("fill"),
    NormalizedHead::new("reduce"),
    NormalizedHead::new("parse-integer"),
    NormalizedHead::new("read-from-string"),
    NormalizedHead::new("string-upcase"),
    NormalizedHead::new("string-downcase"),
    NormalizedHead::new("string-capitalize"),
    NormalizedHead::new("nstring-upcase"),
    NormalizedHead::new("nstring-downcase"),
    NormalizedHead::new("nstring-capitalize"),
    NormalizedHead::new("write-string"),
    NormalizedHead::new("write-line"),
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
        let mut call_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut call_form_count, &mut items);
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
                RuleFix::multi(
                    "Drop the redundant :end nil".to_owned(),
                    Replacement::new(item.removal_span, String::new()),
                    [],
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "{} :end defaults to nil; drop the explicit :end nil",
                    item.head
                ),
                fix,
            );
        }
        Ok(())
    }
}
