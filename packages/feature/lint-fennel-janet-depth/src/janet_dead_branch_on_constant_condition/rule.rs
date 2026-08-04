//! `janet-dead-branch-on-constant-condition`: a Janet conditional whose test is
//! a literal, so one branch can never run.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::janet_dead_branch_on_constant_condition::domain::{self, examine};
use crate::support::is_unevaluated_either;

pub const META: RuleMeta = RuleMeta::new(
    "janet-dead-branch-on-constant-condition",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a Janet `if`/`if-not`/`when`/`unless` whose test is a literal, so one branch never runs",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Janet's compiler folds a constant condition and compiles the losing branch into a \
         throwaway scope, reporting `dead code, consider removing …` at its strict lint level \
         (`compile.c`, `janetc_throwaway`). Strict lints are off unless asked for, so the \
         warning normally reaches nobody. In Janet only `nil` and `false` are false, so `0`, \
         `\"\"` and `:kw` are all truthy tests.",
    )
    .with_example("(if true (start) (abort))", "(start)")
    .with_caveat(
        "Only a literal written in place is seen. The real compiler also folds a `def`-bound \
         value, so `(def limit 10) (if limit …)` lints there and not here — Janet has no binding \
         or value table in this engine. Every finding here is one the compiler would also make; \
         the reverse is not true. An explicit `nil` branch is skipped, exactly as the compiler \
         skips it.",
    ),
);

const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("if"),
    NormalizedHead::new("if-not"),
    NormalizedHead::new("when"),
    NormalizedHead::new("unless"),
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
        // After the domain check, never before: the guard materializes the
        // whole document, and every `if` in the file would otherwise pay for
        // it. This guard is doing heavy lifting for this rule in particular:
        // `if`, `if-not` and `not` are also PEG combinators, and Janet's PEG
        // grammars are written as quoted or quasiquoted structs. Removing it
        // takes the third-party sweep from 4 findings to 40.
        if is_unevaluated_either(context.tree(), view.span, item.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "`{}` is constant, so {} of this `{}` can never run",
                item.condition,
                item.branch.describe(),
                item.head
            ),
        );
        Ok(())
    }
}
