//! `with-accessors-empty-binding-list`: a with-slots/with-accessors with an empty binding list.
//!

use paredit_core_lint_engine::LintResult;

use crate::support::is_unevaluated_at;
use crate::with_accessors_empty_binding_list::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "with-accessors-empty-binding-list",
    // Well-formed code whose meaning is probably not what was intended: the
    // form runs, binds nothing, and is a `progn` written the long way — the
    // same reading `empty-let` gets.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a with-slots/with-accessors with an empty binding list, which is just progn ((with-slots () o body) is (progn o body))",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 2] = [
    NormalizedHead::new("with-accessors"),
    NormalizedHead::new("with-slots"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first, and the quote descent last.
    ///
    /// [`examine`] is pure list-shape reading of the matched node — a head
    /// comparison, a length check, one child's kind. Only when it has produced
    /// a candidate is [`is_unevaluated_at`] asked, so a file full of
    /// `with-slots` forms with real bindings never pays for a single descent.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut binding_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut binding_form_count, &mut items);
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
