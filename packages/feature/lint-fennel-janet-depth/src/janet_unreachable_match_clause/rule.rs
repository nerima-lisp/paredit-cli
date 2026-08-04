//! `janet-unreachable-match-clause`: a `match` clause after a catch-all
//! pattern.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::janet_unreachable_match_clause::domain::{self, examine};
use crate::support::is_unevaluated_either;

pub const META: RuleMeta = RuleMeta::new(
    "janet-unreachable-match-clause",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a Janet `match` clause written after a catch-all pattern, which can never be reached",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "Janet's `match` documents that \"a pattern that is a symbol will match anything\", and \
         `_` is the same with no binding. Every clause after such a pattern is dead: \
         `(match 99 x :first 99 :second)` evaluates to `:first` even though the later clause \
         matches the subject exactly. The usual cause is a default clause that drifted upwards, \
         or a pattern renamed into a bare symbol by accident.",
    )
    .with_example(
        "(match code x :unknown 404 :not-found)",
        "(match code 404 :not-found x :unknown)",
    )
    .with_caveat(
        "A catch-all in the *last* pattern position is the idiomatic default clause and is never \
         reported. A quoted symbol (`'foo`) is a literal to compare against, and a tuple pattern \
         such as `(x (> x 5))` or the `(@ pinned)` form is a guard, so neither is a catch-all. \
         Janet's own compiler does not report this — `match` expands to nested `if`s over a \
         gensym, so nothing is constant-folded — which is why the finding is worth surfacing.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("match")];

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
        // whole document, and every `match` in the file would otherwise pay
        // for it. See `is_unevaluated_either` for why both spans are asked
        // about.
        if is_unevaluated_either(context.tree(), view.span, item.span) {
            return Ok(());
        }
        let also = match item.shadowed {
            0 | 1 => String::new(),
            2 => " and the one after it".to_owned(),
            more => format!(" and the {} after it", more - 1),
        };
        sink.report(
            item.span,
            format!(
                "the earlier pattern `{}` matches anything, so this clause{also} can never run",
                item.catch_all
            ),
        );
        Ok(())
    }
}
