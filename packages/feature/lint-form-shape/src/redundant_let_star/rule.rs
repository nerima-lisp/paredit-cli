//! `redundant-let-star`: a let* with zero or one binding, which is just let (no sequential scope in play).
//!

use paredit_core_lint_engine::LintResult;

use crate::redundant_let_star::domain::examine_let_star;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "redundant-let-star",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let* with zero or one binding, which is just let (no sequential scope in play)",
    Fixability::Fixable,
);

/// `examine_let_star` only ever matches a `let*` head.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("let*")];

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
        let mut let_star_form_count = 0;
        let mut items = Vec::new();
        examine_let_star(view, &mut let_star_form_count, &mut items);
        for item in items {
            let span = item.span;
            // A rewrite of a form inside `'(…)` or `(quote …)` edits a
            // *data literal*, not code, so the finding is dropped rather
            // than fixed. Read on the `hard` counter alone: a `` `(…) ``
            // template's contents really are emitted as code, and going
            // quiet there would abandon the macro bodies this rule exists
            // to read. Asked once per finding, never per visited node.
            if is_hard_quoted_at(context.tree(), span) {
                continue;
            }
            let fix = {
                // Rewrite just the head symbol: a ≤1-binding let* is exactly let.

                RuleFix::single(
                    item.head_span,
                    "let".to_owned(),
                    "Rewrite the redundant let* as let".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                format!(
                    "let* with {} binding{} is just let; sequential scope is unused",
                    item.binding_count,
                    if item.binding_count == 1 { "" } else { "s" }
                ),
                fix,
            );
        }
        Ok(())
    }
}
