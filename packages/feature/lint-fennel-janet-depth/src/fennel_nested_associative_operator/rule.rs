//! `fennel-nested-associative-operator`: `(and a (and b c))`, which is
//! `(and a b c)` with an extra level of parentheses.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::fennel_nested_associative_operator::domain::{self, examine};
use crate::support::is_unevaluated_either;

pub const META: RuleMeta = RuleMeta::new(
    "fennel-nested-associative-operator",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Fennel `and`/`or`/`..`/`band`/`bor` call nested directly inside a call to itself",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "These five operators are variadic and exactly associative, so `(and foo (and bar baz))` \
         and `(and foo bar baz)` compute the same value by the same short-circuit order. The \
         nested spelling is usually left over from an edit that added a term. `fennel-ls` ships \
         this as `nested-associative-operator`.",
    )
    .with_example(
        "(and foo bar (and baz buzz) xyz)",
        "(and foo bar baz buzz xyz)",
    )
    .with_caveat(
        "`+` and `*` are excluded, though `fennel-ls` includes them. Lua's numbers are IEEE-754 \
         doubles and floating-point addition and multiplication are not associative: \
         `(* 1e300 (* 1e300 1e-300))` is `1e+300` while the collapsed `(* 1e300 1e300 1e-300)` \
         is `inf`. Advising that collapse would be advising an overflow.",
    ),
);

const HEADS: [NormalizedHead; 5] = [
    NormalizedHead::new("and"),
    NormalizedHead::new("or"),
    NormalizedHead::new(".."),
    NormalizedHead::new("band"),
    NormalizedHead::new("bor"),
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
        // whole document, and every `and` in the file would otherwise pay for
        // it. See `is_unevaluated_either` for why both spans are asked about.
        if is_unevaluated_either(context.tree(), view.span, item.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "`{}` is variadic and associative, so this nested call can be spliced into the \
                 outer one",
                item.operator
            ),
        );
        Ok(())
    }
}
