//! `flet-single-use-inlinable`: an flet/labels whose one local function is called once, in tail position.
//!

use paredit_core_lint_engine::LintResult;

use crate::flet_single_use_inlinable::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "flet-single-use-inlinable",
    // Correct code with a name that buys nothing: the same "well-formed, but
    // probably not what was meant to stay" reading `redundant-funcall` and
    // `funcall-lambda` get.
    RuleCategory::Suspicious,
    Severity::Warning,
    "an flet/labels defining one local function whose only use is a tail call that is the whole body",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("flet"), NormalizedHead::new("labels")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first. [`examine`] rejects on the form's own child
    /// count, then the binding list's length, before reading any lambda list;
    /// the whole-form occurrence count runs only after every structural check
    /// has passed, and the quote descent only after that.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut single_binding_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut single_binding_form_count, &mut items);
        if items.is_empty() || is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            let span = item.span;
            let message = paredit_core_cli::report::Finding::message(&item);
            sink.report(span, message);
        }
        Ok(())
    }
}
