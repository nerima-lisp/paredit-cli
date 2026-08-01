//! `duplicate-defmethod-signature`: two `defmethod`s with the same name,
//! qualifiers and specializers.
//!
//! The analysis lives in [`crate::duplicate_defmethod_signature::domain`],
//! which also backs the standalone `inspect duplicate-defmethod-signature`
//! command; this module only registers it with the lint suite and phrases its
//! findings.

use paredit_core_lint_engine::LintResult;

use crate::duplicate_defmethod_signature::domain::examine_duplicate_defmethod_signature;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "duplicate-defmethod-signature",
    // `Duplicate` rather than `ObjectSystem`: the category is "the same key,
    // place, test, or name given twice", and a method signature is exactly the
    // key CLOS files a method under.
    RuleCategory::Duplicate,
    // An error: unlike the other rules here this one has no benign reading.
    // One of the two bodies provably never runs.
    Severity::Error,
    "two defmethods with the same name, qualifiers and specializers",
    // No fix. Deleting the earlier definition is what the running image already
    // does, but the later one may be the accident — the repair depends on which
    // body was meant to survive.
    Fixability::ReportOnly,
);

/// `examine_duplicate_defmethod_signature` only ever matches a `defmethod`
/// head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

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
        let mut defmethod_form_count = 0;
        let mut items = Vec::new();
        examine_duplicate_defmethod_signature(
            context.tree(),
            view,
            &mut defmethod_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "this defmethod on {} repeats an earlier one's signature ({}): CLOS \
                     replaces the earlier method rather than adding a second, so its body \
                     never runs",
                    item.generic, item.signature
                ),
            );
        }
        Ok(())
    }
}
