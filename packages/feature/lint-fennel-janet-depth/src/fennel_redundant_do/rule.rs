//! `fennel-redundant-do`: a `(do …)` in the tail of a form that already has an
//! implicit `do`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::fennel_redundant_do::domain::{self, examine};
use crate::support::is_unevaluated_either;

pub const META: RuleMeta = RuleMeta::new(
    "fennel-redundant-do",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a `(do …)` in the tail of a Fennel form that already sequences its body",
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "`fn`, `let`, `when`, `each`, `for`, `while` and friends already evaluate every form in \
         their body in order, so wrapping the last one in `do` adds a level of nesting and no \
         meaning. `fennel-ls` ships this as `redundant-do`.",
    )
    .with_example(
        "(fn [] (do (print :first) (print :second)))",
        "(fn [] (print :first) (print :second))",
    )
    .with_caveat(
        "The head list is deliberately shorter than `fennel-ls`'s. That lint uses every form \
         `(fennel.syntax)` marks `body-form?`, but nine of those accept exactly one body \
         expression — `accumulate`, `faccumulate`, `fcollect`, `icollect` answer \"expected \
         exactly one body expression. Wrap multiple expressions in do\", and `case`, `case-try`, \
         `match`, `match-try` take pattern/body pairs. In all nine the `do` is load-bearing and \
         removing it does not compile, so they are excluded here. `doto` applies its trailing \
         forms to the object rather than sequencing them, and `comment` discards its contents; \
         both are excluded too.",
    ),
);

const HEADS: [NormalizedHead; 13] = [
    NormalizedHead::new("collect"),
    NormalizedHead::new("do"),
    NormalizedHead::new("each"),
    NormalizedHead::new("eval-compiler"),
    NormalizedHead::new("fn"),
    NormalizedHead::new("for"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("let"),
    NormalizedHead::new("macro"),
    NormalizedHead::new("when"),
    NormalizedHead::new("while"),
    NormalizedHead::new("with-open"),
    NormalizedHead::new("λ"),
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
        // whole document, and every `fn` in the file would otherwise pay for
        // it. The *reported* span is the one that matters here: eleven of the
        // thirteen raw hits in the third-party sweep were
        // `` (macro m [x] `(do ,x ,x)) ``, where the dispatched `(macro …)` is
        // ordinary code and the `(do …)` is a list the macro constructs.
        if is_unevaluated_either(context.tree(), view.span, item.span) {
            return Ok(());
        }
        sink.report(
            item.span,
            format!(
                "`{}` already evaluates its body in order, so this `do` only adds nesting",
                item.outer
            ),
        );
        Ok(())
    }
}
