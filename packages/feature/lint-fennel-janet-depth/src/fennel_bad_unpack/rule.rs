//! `fennel-bad-unpack`: an `unpack` call the operator around it will truncate.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::fennel_bad_unpack::domain::{self, advice_for, examine};
use crate::support::is_unevaluated_either;

pub const META: RuleMeta = RuleMeta::new(
    "fennel-bad-unpack",
    RuleCategory::Suspicious,
    Severity::Warning,
    "an `unpack` call in the last argument of a Fennel operator, which drops every value but the \
     first",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Fennel's operators compile to Lua's binary operators, not to variadic function calls, \
         and Lua truncates a multiple-value expression to a single value everywhere except the \
         final argument of a real call. So `(+ 1 (table.unpack [2 3 4]))` compiles to \
         `(1 + table.unpack({2, 3, 4}))` and evaluates to 3, not 10 — the 3 and the 4 are \
         discarded with no error. `fennel-ls` ships this as `bad-unpack`.",
    )
    .with_example(
        "(+ 1 (table.unpack [2 3 4]))",
        "(accumulate [sum 1 _ n (ipairs [2 3 4])] (+ sum n))",
    )
    .with_caveat(
        "A one-argument call to `..`, `and`, `or`, `%` or `^` is *not* reported. Fennel compiles \
         those away entirely — `(.. (table.unpack xs))` becomes a bare `table.unpack(xs)` — so \
         every value survives and there is no defect. `fennel-ls` reports them anyway; that is \
         the one place this rule deliberately says less than its source.",
    ),
);

const HEADS: [NormalizedHead; 23] = [
    NormalizedHead::new("+"),
    NormalizedHead::new("-"),
    NormalizedHead::new("*"),
    NormalizedHead::new("/"),
    NormalizedHead::new("//"),
    NormalizedHead::new("%"),
    NormalizedHead::new("^"),
    NormalizedHead::new(">"),
    NormalizedHead::new("<"),
    NormalizedHead::new(">="),
    NormalizedHead::new("<="),
    NormalizedHead::new("="),
    NormalizedHead::new("not="),
    NormalizedHead::new(".."),
    NormalizedHead::new("."),
    NormalizedHead::new("and"),
    NormalizedHead::new("or"),
    NormalizedHead::new("band"),
    NormalizedHead::new("bor"),
    NormalizedHead::new("bxor"),
    NormalizedHead::new("bnot"),
    NormalizedHead::new("lshift"),
    NormalizedHead::new("rshift"),
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
        // whole document, and every `+` in the file would otherwise pay for
        // it. Both spans are asked about — `` `(and ,c ,(unpack gs)) `` in
        // Fennel's own `match.fnl` is code at the reported node and a template
        // at the form, and only the form span rejects it.
        if is_unevaluated_either(context.tree(), view.span, item.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "`{}` is not variadic at runtime, so this `{}` yields only its first value; {}",
                item.operator,
                item.unpack,
                advice_for(&item.operator)
            ),
        );
        Ok(())
    }
}
