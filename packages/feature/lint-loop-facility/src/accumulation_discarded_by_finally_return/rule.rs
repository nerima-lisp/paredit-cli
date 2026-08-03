//! `loop-accumulation-discarded-by-finally-return`: a `loop` whose implicit
//! accumulation a `finally (return …)` throws away.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::accumulation_discarded_by_finally_return::domain::examine;
use crate::shared::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "loop-accumulation-discarded-by-finally-return",
    // The accumulated value is computed on every iteration and read by
    // nothing. That is dead code with a per-iteration cost, not a shape defect.
    RuleCategory::DeadCode,
    // `Warning`, not `Error`: the program is well-formed and does what it says.
    // What it loses is the work, and — for `collect`/`append`/`nconc` — the
    // consing that work does.
    Severity::Warning,
    "a loop whose implicit accumulation is discarded by a `finally (return …)`",
    // Two repairs exist — delete the accumulation clause, or delete the
    // `finally` return — and they produce different programs. Which one the
    // author meant is exactly the intent a machine cannot infer.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "An extended `loop` has at most one implicit result, produced by the accumulation \
         clauses that name no `into` variable (CLHS 6.1.1.3). A `finally (return …)` returns \
         from the loop's implicit `nil` block and pre-empts that result (CLHS 6.1.2.3) — and \
         because an implicit accumulation has no name, the `finally` can never be returning it. \
         So the accumulated value is built on every iteration and read by nothing. Measured \
         under SBCL 2.6.0, `(loop for x in '(1 2 3) collect x finally (return :other))` returns \
         `:OTHER` after fully consing `(1 2 3)`, and no warning is emitted.",
    )
    .with_example(
        "(loop for x in items\n      collect (expensive x)\n      finally (return :done))",
        "(loop for x in items\n      do (expensive x)\n      finally (return :done))",
    )
    .with_caveat(
        "An accumulation with an `into` target is never reported — the value has a name and the \
         `finally` may be returning something computed from it. Nor is a `named` loop, where \
         `return` targets the named block rather than the loop; nor `return-from`; nor a \
         conditional `finally (when p (return v))`, which discards the accumulation on only \
         some paths.",
    ),
);

/// `examine` reads nothing but a `loop` form.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("loop")];

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
    ) -> LintResult {
        // Cheapest first: this reads only the form's own top-level children.
        let found = examine(view);
        if found.is_empty() {
            return Ok(());
        }
        // Only now, with a finding otherwise ready: this descends from
        // `root_view`, which materializes the whole document.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in found {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// See the sibling rule's note: the head index is what keeps this off
    /// every file with no `loop` in it, and `clean/forms/*` measures exactly
    /// that.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["loop"]);
    }
}
