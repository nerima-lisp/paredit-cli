//! `list-star-nil`: a list* with a nil tail, a spelled-out list ((list* a b nil) is (list a b)).
//!

use paredit_core_lint_engine::LintResult;

use crate::list_star_nil::domain::examine;
use crate::support::is_hard_quoted_at;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, Replacement, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "list-star-nil",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a list* with a nil tail, a spelled-out list ((list* a b nil) is (list a b))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("list*")];

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
                // Two disjoint edits: rewrite the head to list and drop the nil tail.
                let fix_first = Replacement::new(item.head_span, "list".to_owned());
                let fix_rest = [Replacement::new(item.removal_span, String::new())];

                RuleFix::multi(
                    "Rewrite list* with a nil tail as list".to_owned(),
                    fix_first,
                    fix_rest,
                )
            };

            sink.report_fixed(
                span,
                "list* with a nil tail is a spelled-out list; (list* a b nil) is (list a b)"
                    .to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
