//! `loop-parallel-binding-reads-sibling`: a `loop` variable clause joined by
//! `and` whose initial value reads a variable bound in the same parallel group.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::parallel_binding_reads_sibling::domain::examine;
use crate::shared::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "loop-parallel-binding-reads-sibling",
    // Not `Malformed`: the shape is legal `loop`, and in the everyday case it
    // compiles without a murmur and returns a wrong answer. What is wrong is
    // the meaning, which is what `Suspicious` is for.
    RuleCategory::Suspicious,
    // `Error` rather than `Warning` because the two outcomes measured are a
    // silently wrong value and a runtime type error; neither is a style
    // preference.
    Severity::Error,
    "a loop clause joined by `and` whose initial value reads a variable bound in parallel with it",
    // The obvious repair — replace `and` with a second `for` — changes
    // sequential-versus-parallel semantics for *every* variable in the group,
    // not only the one that reads. When the group also contains a `then` step
    // that relies on parallel binding, that edit silently breaks it. Report
    // only.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Successive `for`/`as`/`with` clauses bind sequentially, like `let*`; clauses joined by \
         `and` bind simultaneously, like `let` (CLHS 6.1.1.4). So a clause's initial value cannot \
         read a variable bound in the same `and` group — at that moment the sibling still holds \
         `nil`, or is not bound at all. Measured under SBCL 2.6.0, \
         `(loop for a from 1 to 3 and b = (* a 10) collect (list a b))` returns \
         `((1 10) (2 10) (3 20))` with no warning of any kind, where the author meant \
         `((1 10) (2 20) (3 30))`; the `with` spelling instead fails outright with an \
         UNBOUND-VARIABLE error.",
    )
    .with_example(
        "(loop for a from 1 to 3\n      and b = (* a 10)\n      collect (list a b))",
        "(loop for a from 1 to 3\n      for b = (* a 10)\n      collect (list a b))",
    )
    .with_caveat(
        "A sibling read in a `then` step form is never reported: that is the standard \
         \"previous element\" idiom and it requires `and`. Measured under SBCL 2.6.0, \
         `(loop for x in '(1 2 3) and prev = nil then x collect (cons prev x))` correctly \
         yields `((NIL . 1) (1 . 2) (2 . 3))`, while the sequential spelling is the one that \
         gets it wrong. A read inside a nested `let`, `lambda`, `destructuring-bind` or `loop` \
         is also not reported, because the inner form may shadow the name.",
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
        // Cheapest first. `examine` reads only this form's own children, and
        // bails on the first token for anything that is not an extended `loop`.
        let found = examine(view);
        if found.is_empty() {
            return Ok(());
        }
        // Only now, with a finding otherwise ready: this one descends from
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

    /// The engine's head index is what keeps this rule off every file with no
    /// `loop` form in it. `AllNodes` would cost one call per node and
    /// `WholeTree` one pass per file, both paid even when the rule matches
    /// nothing — which is precisely what the `clean/forms/*` benchmarks
    /// measure. Pinned here so the declaration cannot drift.
    #[test]
    fn the_rule_is_reached_only_through_its_head() {
        assert_eq!(RULE.head_filter(), HeadFilter::Heads(&HEADS));
        assert_eq!(HEADS.map(NormalizedHead::as_str), ["loop"]);
    }
}
