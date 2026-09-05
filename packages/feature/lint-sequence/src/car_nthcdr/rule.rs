//! `car-nthcdr`: a car of an nthcdr, which is nth ((car (nthcdr n x)) is (nth n x)).
//!

use paredit_core_lint_engine::LintResult;

use crate::car_nthcdr::domain::examine;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleFix, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "car-nthcdr",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a car of an nthcdr, which is nth ((car (nthcdr n x)) is (nth n x))",
    Fixability::Fixable,
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("car")];

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
        let context_slice = |span| context.slice(span).to_owned();
        let mut car_form_count = 0;
        let mut items = Vec::new();
        examine(view, &mut car_form_count, &mut items);
        for item in items {
            let span = item.span;
            let fix = {
                // (car (nthcdr n x)) is (nth n x).
                let text = format!(
                    "(nth {} {})",
                    context_slice(item.count_span),
                    context_slice(item.list_span)
                );

                // The fix region is `content_span`, not `span`: `span` starts at this
                // form's *own* reader prefixes, so replacing it deletes them. A
                // `` `(…) `` has to keep its backquote — without it the commas
                // underneath are commas outside a backquote, and the file stops
                // reading altogether. The two spans coincide on any form with no
                // prefix, which is almost all code, so nothing else moves.
                //
                // This rule is deliberately not hard-quote guarded (see
                // `quote_guard_tests`), so it is the one rule here that also
                // reaches `'(car (nthcdr n x))` — where replacing `span` deleted
                // the `'` and turned a quoted datum into a live call.
                RuleFix::single(
                    view.content_span,
                    text,
                    "Rewrite (car (nthcdr n x)) as (nth n x)".to_owned(),
                )
            };

            sink.report_fixed(
                span,
                "car of an nthcdr is nth; (car (nthcdr n x)) is (nth n x)".to_owned(),
                fix,
            );
        }
        Ok(())
    }
}
