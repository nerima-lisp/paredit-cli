//! Registration for `type-declaration-on-rest-parameter`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};
use crate::type_declaration_on_rest_parameter::domain::examine_form;

pub const META: RuleMeta = RuleMeta::new(
    "type-declaration-on-rest-parameter",
    RuleCategory::Declaration,
    // SBCL emits a full WARNING: the declared type and the binding mechanism
    // cannot both be right.
    Severity::Warning,
    "a &rest parameter declared to be its element type, though it is always bound to a list",
    // The author wanted to say something about the *elements*, which a &rest
    // declaration cannot express at all; there is no mechanical rewrite that
    // says it.
    Fixability::ReportOnly,
);

/// Only the three heads whose lambda list sits at a fixed index. `defmethod` is
/// deliberately absent — see the domain module.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("lambda"),
];

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
        let items = examine_form(view);
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
