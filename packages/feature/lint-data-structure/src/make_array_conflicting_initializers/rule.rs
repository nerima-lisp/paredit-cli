//! `make-array-conflicting-initializers`: a `make-array` supplying both
//! `:initial-element` and `:initial-contents`.
//!
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::make_array_conflicting_initializers::domain::examine_make_array_conflicting_initializers;

pub const META: RuleMeta = RuleMeta::new(
    "make-array-conflicting-initializers",
    RuleCategory::Malformed,
    // SBCL refuses the call outright rather than choosing one, so every array
    // this allocates is an error at runtime, not a stylistic complaint.
    Severity::Error,
    "a make-array supplying both :initial-element and :initial-contents",
    // No fix. Deleting either keyword produces a legal call, and which one the
    // author meant is the whole question — `:initial-contents '(1 2 3)` and
    // `:initial-element 0` build different arrays.
    Fixability::ReportOnly,
);

/// `examine_make_array_conflicting_initializers` only matches a `make-array`.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("make-array")];

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
        let mut make_array_form_count = 0;
        let mut items = Vec::new();
        examine_make_array_conflicting_initializers(
            context.tree(),
            view,
            &mut make_array_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
