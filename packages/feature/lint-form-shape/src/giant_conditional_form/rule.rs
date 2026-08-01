//! `giant-conditional-form`: a `let`/`let*`/`cond`/`case`-family form
//! carrying more bindings or clauses than a configurable threshold.
//!
//! The analysis lives in [`crate::giant_conditional_form::domain`], which also
//! backs the standalone `inspect giant-conditional-form` command; this module
//! only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::giant_conditional_form::domain::{DEFAULT_THRESHOLD, examine};
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleSetting,
    Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

/// The knob: how many bindings (`let`/`let*`) or clauses (everything else) a
/// form may carry before this rule speaks.
pub const MAX_CLAUSES: RuleSetting = RuleSetting::new(
    "max-clauses",
    DEFAULT_THRESHOLD as i64,
    "how many bindings or clauses a let/cond/case-family form may carry before it is reported",
);

pub const META: RuleMeta = RuleMeta::new(
    "giant-conditional-form",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a let/let*/cond/case-family form carrying more bindings or clauses than a threshold",
    Fixability::ReportOnly,
)
.with_settings(&[MAX_CLAUSES])
.with_explanation(
    RuleExplanation::new(
        "A form with many bindings or clauses is doing many things in one place. Splitting it — \
         into smaller lets, a dispatch table, or separate functions per clause — usually makes \
         each piece independently readable and testable, at the cost of naming the pieces.",
    )
    .with_example(
        "(cond (a 1) (b 2) (c 3) (d 4) (e 5) (f 6) (g 7) (h 8) (i 9))",
        "(dispatch-table x)",
    )
    .with_caveat(
        "The threshold is a size heuristic, not a correctness judgement. A `case` with twenty \
         short, mechanical clauses can be more readable than five long ones.",
    ),
);

const HEADS: [NormalizedHead; 9] = [
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("cond"),
    NormalizedHead::new("case"),
    NormalizedHead::new("ecase"),
    NormalizedHead::new("ccase"),
    NormalizedHead::new("typecase"),
    NormalizedHead::new("etypecase"),
    NormalizedHead::new("ctypecase"),
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
        let threshold = context.setting(META.name().as_str(), MAX_CLAUSES);
        let mut scanned_form_count = 0;
        let mut items = Vec::new();
        examine(
            view,
            context.path(),
            threshold.max(0) as usize,
            &mut scanned_form_count,
            &mut items,
        );
        for item in items {
            sink.report(
                item.span,
                format!(
                    "{} carries {} {}, more than the {} allowed before splitting",
                    item.head,
                    item.count,
                    if item.head.starts_with("let") {
                        "bindings"
                    } else {
                        "clauses"
                    },
                    item.threshold,
                ),
            );
        }
        Ok(())
    }
}
