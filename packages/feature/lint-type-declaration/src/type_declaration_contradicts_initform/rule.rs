//! Registration for `type-declaration-contradicts-initform`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};
use crate::type_declaration_contradicts_initform::domain::examine_let;

pub const META: RuleMeta = RuleMeta::new(
    "type-declaration-contradicts-initform",
    RuleCategory::Declaration,
    // SBCL emits a full WARNING: the declaration is a promise the binding
    // breaks, and an optimising compiler is entitled to believe it.
    Severity::Warning,
    "a let binding whose declared type cannot contain the literal it is initialised to",
    // Which half is wrong — the type or the initial value — is the author's
    // call, and neither repair is right more often than the other.
    Fixability::ReportOnly,
);

/// Only the two binding forms that pair a name with an initial value in a shape
/// this rule can read. `flet` and `labels` bind functions, `do` puts its step
/// form where an initform is not, and `multiple-value-bind` has no per-variable
/// initial value at all.
const HEADS: [NormalizedHead; 2] = [NormalizedHead::new("let"), NormalizedHead::new("let*")];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        COMMON_LISP_ONLY
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        let items = examine_let(view);
        if items.is_empty() {
            return Ok(());
        }
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
