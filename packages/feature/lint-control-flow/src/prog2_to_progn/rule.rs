//! `prog2-to-progn`: a two-form prog2, which is just progn ((prog2 a b) is (progn a b)).
//!

use paredit_core_lint_engine::LintResult;

use crate::prog2_to_progn::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "prog2-to-progn",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a two-form prog2, which is just progn ((prog2 a b) is (progn a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("prog2")];

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
        let mut prog2_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut prog2_form_count, &mut items);
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
                // Rewrite the operator token prog2 -> progn, keeping the two forms.

                RuleFix::single(
                    item.head_span,
                    "progn".to_owned(),
                    "Rewrite prog2 as progn".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "a two-form prog2 is just progn; (prog2 a b) is (progn a b)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
