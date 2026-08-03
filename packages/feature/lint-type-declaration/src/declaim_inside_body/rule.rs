//! Registration for `declaim-inside-body`.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::declaim_inside_body::domain::examine_body;
use crate::support::{COMMON_LISP_ONLY, is_unevaluated_at};

pub const META: RuleMeta = RuleMeta::new(
    "declaim-inside-body",
    RuleCategory::Declaration,
    // SBCL emits a STYLE-WARNING rather than an error: the form is legal, it
    // just does something global that the author almost certainly did not want.
    Severity::Warning,
    "a (declaim ...) among a body's leading declarations, where (declare ...) was meant",
    // Rewriting `declaim` to `declare` is usually right and occasionally
    // destructive: an author who really did want a global proclamation at load
    // time would have it silently turned into a local declaration.
    Fixability::ReportOnly,
);

/// [`DECLARATION_BODY_RULE_HEADS`] **minus** `locally`, `macrolet` and
/// `symbol-macrolet`.
///
/// CLHS 3.2.3.1 ("Processing of Top Level Forms") says the body forms of a
/// top-level `progn`, `locally`, `macrolet` or `symbol-macrolet` are themselves
/// processed as top level forms. A `declaim` in one of those bodies is therefore
/// an ordinary top-level proclamation, correctly spelled, and SBCL's own sources
/// rely on it — the corpus audit found both
///
/// ```lisp
/// (locally (declare (optimize (speed 3) (safety 0)))
///   (declaim (inline %constraint-number))
///   (defun %constraint-number (constraint) ...))
/// ```
///
/// in `sbcl/src/compiler/constraint.lisp` and a `macrolet` doing the same in
/// `sbcl/src/code/target-unicode.lisp`. Both were false positives.
///
/// A `locally` *nested inside a function* is not a top-level context and a
/// `declaim` there really is the confusion this rule is about, but telling the
/// two apart needs the parent chain, and the nested case is rare enough that
/// dropping the heads outright is the better trade.
///
/// [`DECLARATION_BODY_RULE_HEADS`]: crate::support::DECLARATION_BODY_RULE_HEADS
const HEADS: [NormalizedHead; 19] = [
    NormalizedHead::new("defun"),
    NormalizedHead::new("defmacro"),
    NormalizedHead::new("define-compiler-macro"),
    NormalizedHead::new("deftype"),
    NormalizedHead::new("lambda"),
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
    NormalizedHead::new("prog"),
    NormalizedHead::new("prog*"),
    NormalizedHead::new("multiple-value-bind"),
    NormalizedHead::new("destructuring-bind"),
    NormalizedHead::new("with-slots"),
    NormalizedHead::new("with-accessors"),
    NormalizedHead::new("do"),
    NormalizedHead::new("do*"),
    NormalizedHead::new("dolist"),
    NormalizedHead::new("defmethod"),
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
        let items = examine_body(view);
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
