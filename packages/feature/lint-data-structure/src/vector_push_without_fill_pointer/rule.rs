//! `vector-push-without-fill-pointer`: a `vector-push`/`vector-push-extend` on
//! a vector this file made without a `:fill-pointer`.
//!

use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::vector_push_without_fill_pointer::domain::examine_vector_push_without_fill_pointer;

pub const META: RuleMeta = RuleMeta::new(
    "vector-push-without-fill-pointer",
    RuleCategory::Malformed,
    // The call cannot work at any size; it is a type error every time it runs.
    Severity::Error,
    "a vector-push on a vector made with no :fill-pointer",
    // A fix is plausible — insert `:fill-pointer 0` into the make-array — but
    // it edits a *different* form from the one reported, and the right initial
    // value is 0 only when the vector starts empty. `:initial-contents` or a
    // non-zero dimension both mean something else.
    Fixability::ReportOnly,
);

/// The two operators CLHS defines on a vector-with-fill-pointer.
const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("vector-push"),
    NormalizedHead::new("vector-push-extend"),
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
        let mut push_form_count = 0;
        let mut items = Vec::new();
        // Deferred, not evaluated here: see the note in `domain`. Passing
        // `context.binding_table()` directly builds the whole-file semantic
        // table on every `vector-push` in the file, and measured 9431602
        // ns/invocation when this rule was first written that way.
        examine_vector_push_without_fill_pointer(
            context.tree(),
            || context.binding_table(),
            view,
            &mut push_form_count,
            &mut items,
        );
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
