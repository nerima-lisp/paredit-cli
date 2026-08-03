//! `janet-empty-loop-body`: a Janet `loop`/`seq`/`catseq` with a head and no
//! body.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::janet_empty_loop_body::domain::{self, examine};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "janet-empty-loop-body",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Janet `loop`, `seq`, or `catseq` whose body is empty",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Janet's own `loop`, `seq` and `catseq` macros call `check-empty-body`, which emits \
         `\"empty loop body\"` through `maclintf`. That warning only appears when the macro is \
         expanded, so it never reaches a reader of the source. The usual cause is a body that \
         was deleted, or one that ended up outside the closing parenthesis.",
    )
    .with_example("(loop [x :in xs])", "(loop [x :in xs] (print x))")
    .with_caveat(
        "Only the three heads Janet itself guards are covered. `each`, `while`, `for` and \
         `tabseq` have no such check in `boot.janet`, and adding them would be this rule's \
         opinion rather than the language's. In the other direction, a head containing \
         `:iterate` is *not* reported even though Janet's check does report it: `:iterate` \
         evaluates its expression for effect on every round, so an empty body is the point.",
    ),
);

const HEADS: [NormalizedHead; 3] = [
    NormalizedHead::new("loop"),
    NormalizedHead::new("seq"),
    NormalizedHead::new("catseq"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&domain::DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        let Some(item) = examine(context.dialect(), view) else {
            return Ok(());
        };
        // After the domain check, not before: the guard materializes the whole
        // document, and every `loop` in the file would pay for it.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "{} has a head and no body, so it iterates and does nothing",
                item.head
            ),
        );
        Ok(())
    }
}
