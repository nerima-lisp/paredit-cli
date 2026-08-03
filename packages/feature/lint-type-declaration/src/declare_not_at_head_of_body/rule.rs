//! Registration for `declare-not-at-head-of-body`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::declare_not_at_head_of_body::domain::examine_body;
use crate::support::{COMMON_LISP_ONLY, DECLARATION_BODY_RULE_HEADS, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "declare-not-at-head-of-body",
    // Not `Declaration`: the form is not a bad declaration, it is not a
    // declaration at all. SBCL calls it a call to an undefined function.
    RuleCategory::Malformed,
    // SBCL 2.6.0 emits a full `caught ERROR`; see the domain module.
    Severity::Error,
    "a (declare ...) after the first body form, where it is a call to an undefined function",
    // Moving the declaration to the head of the body is usually right, but not
    // always: an author who wrote it late may have meant a `the` or a `check-type`
    // on a value computed by the forms above it, and silently hoisting the
    // declaration would then assert something about a different value.
    Fixability::ReportOnly,
);

const HEADS: [NormalizedHead; 22] = DECLARATION_BODY_RULE_HEADS;

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
        let items = examine_body(view);
        if items.is_empty() {
            return Ok(());
        }
        // Asked once per candidate, after a finding is already in hand: a
        // `(defun …)` inside `'(…)` is a list of symbols.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}
