//! `ftype-values-arity-mismatch`: a declaimed ftype promising more values than its defun returns.
//!

use paredit_core_lint_engine::LintResult;

use crate::ftype_values_arity_mismatch::domain::examine;
use crate::support::is_unevaluated_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "ftype-values-arity-mismatch",
    // Exactly the category's own words: "`declare` / `declaim` that
    // contradicts itself, the lambda list, or the body".
    RuleCategory::Declaration,
    // A violated ftype is undefined behaviour at low safety, and SBCL raises a
    // full WARNING rather than a style-warning for it.
    Severity::Error,
    "a declaimed ftype whose (values ...) return arity is larger than the arity its defun's final literal (values ...) returns",
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("declaim")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    /// Cheapest predicate first, and here the ordering is what keeps the rule
    /// linear: [`examine`] reads the whole `ftype`/`function`/`(values …)`
    /// shape out of the matched node before touching the tree at all, so a
    /// `(declaim (optimize speed))` costs three comparisons and no lookup. Only
    /// a declaim with a fixed-arity `(values …)` pays for the binary search
    /// over `root_children` and the single neighbouring form it materializes —
    /// never a scan of the file, which is what would make N declaims cost
    /// N × file.
    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let mut ftype_declaration_count = 0;
        let mut items = Vec::new();
        examine(
            context.tree(),
            view,
            &mut ftype_declaration_count,
            &mut items,
        );
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
