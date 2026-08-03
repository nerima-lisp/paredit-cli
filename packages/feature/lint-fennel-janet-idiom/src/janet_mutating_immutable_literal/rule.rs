//! `janet-mutating-immutable-literal`: a Janet mutation applied to a literal
//! Janet cannot mutate.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::janet_mutating_immutable_literal::domain::{self, examine};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "janet-mutating-immutable-literal",
    RuleCategory::Suspicious,
    Severity::Error,
    "a Janet mutating call whose target is a tuple, struct, or string literal",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Janet's mutable containers differ from their immutable twins by one character: `@[…]` \
         is an array and `[…]` a tuple, `@{…}` a table and `{…}` a struct, `@\"…\"` a buffer and \
         `\"…\"` a string. `put`, the `array/*` family and the `buffer/*` family all panic when \
         handed the immutable twin, so a dropped `@` is code that reads correctly and dies on \
         its first call.",
    )
    .with_example("(put {:a 1} :b 2)", "(put @{:a 1} :b 2)")
    .with_caveat(
        "Only a literal written at the call site is judged. `(put state :b 2)` is left alone \
         whatever `state` holds — deciding that is the question the rule declines to answer.",
    ),
);

/// One head per entry of [`domain::MUTATORS`].
const HEADS: [NormalizedHead; 17] = [
    NormalizedHead::new("put"),
    NormalizedHead::new("put-in"),
    NormalizedHead::new("update"),
    NormalizedHead::new("update-in"),
    NormalizedHead::new("array/push"),
    NormalizedHead::new("array/pop"),
    NormalizedHead::new("array/concat"),
    NormalizedHead::new("array/insert"),
    NormalizedHead::new("array/remove"),
    NormalizedHead::new("array/fill"),
    NormalizedHead::new("array/clear"),
    NormalizedHead::new("array/ensure"),
    NormalizedHead::new("array/trim"),
    NormalizedHead::new("buffer/push"),
    NormalizedHead::new("buffer/push-string"),
    NormalizedHead::new("buffer/clear"),
    NormalizedHead::new("buffer/format"),
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
        // document, and every `put`/`array/*`/`buffer/*` call would pay.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "{} needs {} and is given {}, which panics at run time; {}",
                item.head,
                item.requirement.describe(),
                item.literal,
                item.requirement.remedy()
            ),
        );
        Ok(())
    }
}
