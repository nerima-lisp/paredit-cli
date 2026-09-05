//! `multiple-value-setq-arity-mismatch`: a multiple-value-setq whose variable list disagrees with a literal (values ...).
//!

use paredit_core_lint_engine::LintResult;

use crate::multiple_value_setq_arity_mismatch::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "multiple-value-setq-arity-mismatch",
    // Legal code that quietly nils a variable or drops a value: `Suspicious`,
    // not `Arity`, which is reserved for calls the operator cannot accept.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a multiple-value-setq whose variable list is a different length from its literal (values ...) right-hand side",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("multiple-value-setq")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first: [`examine`] reads only the matched node's two
    /// operands, and the quote descent runs only once a mismatch has been
    /// found.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut setq_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut setq_form_count, &mut items);
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
