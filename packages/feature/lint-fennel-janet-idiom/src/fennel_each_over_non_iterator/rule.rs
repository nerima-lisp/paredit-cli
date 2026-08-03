//! `fennel-each-over-non-iterator`: a Fennel `each` handed an uncallable
//! literal where an iterator belongs.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::fennel_each_over_non_iterator::domain::{self, examine};
use crate::support::is_unevaluated_at;

pub const META: RuleMeta = RuleMeta::new(
    "fennel-each-over-non-iterator",
    RuleCategory::Suspicious,
    Severity::Error,
    "a Fennel `each` whose iterator position holds a literal that cannot be called",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`each` compiles to Lua's generic `for … in <iterator> do`, which *calls* the iterator on \
         every round. A table, sequence, string or number literal is not callable, so the loop \
         raises \"attempt to call a table value\" the first time it runs. The collection belongs \
         inside `pairs` or `ipairs`.",
    )
    .with_example(
        "(each [k v {:a 1}] (print k v))",
        "(each [k v (pairs {:a 1})] (print k v))",
    )
    .with_caveat(
        "Only a literal is judged. `(each [k v coll] …)` is left alone however suspicious it \
         looks, because a bare symbol is exactly how a real iterator function is spelled and the \
         reference permits it.",
    ),
);

const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("each")];

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
        // document, and every `each` in the file would pay for it.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "each is given {} where an iterator belongs; Lua's generic for calls it, so wrap \
                 it in {}",
                item.kind.describe(),
                item.kind.suggestion()
            ),
        );
        Ok(())
    }
}
