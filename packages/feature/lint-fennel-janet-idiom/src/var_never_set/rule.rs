//! `var-never-set`: a Fennel or Janet mutable binding nothing ever reassigns.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleExplanation, RuleMeta, RuleTag,
    Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::is_unevaluated_in;
use crate::var_never_set::domain::{self, examine, macro_vocabulary};

pub const META: RuleMeta = RuleMeta::new(
    "var-never-set",
    RuleCategory::Suspicious,
    Severity::Warning,
    "a Fennel or Janet `var` binding that nothing ever assigns to",
    Fixability::ReportOnly,
)
.with_tags(&[RuleTag::Style])
.with_explanation(
    RuleExplanation::new(
        "Fennel's `local`/`var` and Janet's `def`/`var` split immutable from mutable bindings. \
         The mutable spelling exists only so a later assignment is legal, so a `var` with no \
         assignment anywhere in the file states a mutability the code never uses and makes a \
         reader look for a reassignment that is not there. Fennel's own linter plugin reports \
         it as `\"<name> declared as var but never set\"`.",
    )
    .with_example(
        "(var total 0)\n(print total)",
        "(local total 0)\n(print total)",
    )
    .with_caveat(
        "The assignment search covers the whole file and is blind to scope and to quoting, so a \
         `var` assigned only from an unrelated scope, or only inside a macro template, is left \
         alone. So is a `var` handed to a macro this file defines or imports, since a macro can \
         expand into an assignment on any argument. That direction is deliberate: every \
         blindness here hides a finding rather than inventing one. A macro imported by \
         `require-macros`, whose names cannot be enumerated, makes the rule decline the file.",
    ),
);

/// Every binder head [`domain::binder_heads_for`] models, across both
/// dialects. The index is a pre-filter, so a head that belongs to only one of
/// them is harmless here — `check` re-reads the per-dialect table.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("var"), NormalizedHead::new("var-")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        // The same constant the domain's vocabulary tables are written
        // against, so a dialect can never be in scope with no vocabulary or
        // have vocabulary with no scope.
        RuleDialectScope::new(&domain::DIALECTS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult {
        // A *performance* guard, not a correctness one: `examine` re-applies
        // `is_candidate` itself, so deleting this line changes no output and
        // no test fails — a mutation run confirmed exactly that. What it
        // changes is cost. Everything below materializes the document, and
        // without this the head index's over-approximation pays for it.
        if !domain::is_candidate(context.dialect(), view) {
            return Ok(());
        }
        let root = context.tree().root_view();
        // The dispatcher walks into quoted data like any other subtree, so a
        // `(var …)` inside a macro template reaches this rule; it is a
        // template, not a binding. Answered against the root view already in
        // hand rather than through `is_unevaluated_at`, which would build a
        // second one.
        if is_unevaluated_in(&root, view.span) {
            return Ok(());
        }
        let macros = macro_vocabulary(context.dialect(), &root);
        let Some(item) = examine(context.dialect(), &root, &macros, view) else {
            return Ok(());
        };
        sink.report(
            item.span,
            format!(
                "{} is declared with {} but never assigned; {} says what this binding is",
                item.name, item.head, item.immutable_head
            ),
        );
        Ok(())
    }
}
