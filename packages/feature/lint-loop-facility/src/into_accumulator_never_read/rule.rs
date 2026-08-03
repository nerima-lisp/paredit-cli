//! `loop-into-accumulator-never-read`: a `loop` that accumulates `into` a
//! variable nothing reads, so the loop returns nil.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::into_accumulator_never_read::domain::examine;
use crate::shared::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "loop-into-accumulator-never-read",
    // The accumulation runs every iteration and its result is unreachable.
    RuleCategory::DeadCode,
    // `Error` rather than `Warning`: unlike the sibling `discarded-by-finally`
    // rule, where the loop still returns something the author chose, this loop
    // returns `nil` where the author plainly expected a value.
    Severity::Error,
    "a loop accumulation whose `into` variable nothing reads, so the loop returns nil",
    // The repair is either to drop the `into` (making the accumulation the
    // loop's result) or to add a `finally (return …)`. Both are plausible and
    // they are different programs.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Naming an `into` variable takes the accumulation out of the loop's implicit result \
         (CLHS 6.1.3), and the variable's scope ends with the loop. If no `finally`, `do`, or \
         conditional clause reads it, the value cannot escape and the loop returns `nil`. \
         Measured under SBCL 2.6.0, `(loop for x in '(1 2 3) collect x into acc)` returns `NIL`. \
         For a list accumulator SBCL adds a style warning; for a numeric one — \
         `(loop for x in '(1 2 3) sum x into total)` — it emits nothing at all, because \
         `total` is read by its own accumulation, so the compiler has no way to notice.",
    )
    .with_example(
        "(loop for x in items\n      collect (f x) into acc)",
        "(loop for x in items\n      collect (f x) into acc\n      finally (return acc))",
    )
    .with_caveat(
        "One further occurrence of the name anywhere in the loop — at any depth, in any clause \
         — suppresses the finding, so two `into` clauses sharing an accumulator across `when`/\
         `else` branches are never reported. This rule and `lint-form-shape`'s \
         `loop-collect-into-immediately-returned` have disjoint populations by construction: \
         that one requires the accumulator to occur exactly twice, this one exactly once.",
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
        // Cheapest first: `examine` bails on the first token unless the form is
        // an extended `loop` with an `into` clause, and only then walks the
        // subtree to count occurrences.
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

    /// See the sibling rules' note: the head index is what keeps this off every
    /// file with no `loop` in it, and `clean/forms/*` measures exactly that.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["loop"]);
    }
}
