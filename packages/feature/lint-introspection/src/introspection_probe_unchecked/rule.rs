//! `introspection-probe-unchecked`: a probe whose not-found answer is `nil`,
//! applied directly by `funcall`/`apply`.
//!

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, RuleCategory, RuleExplanation, RuleMeta, Severity,
};
use paredit_core_lint_engine::policy::RuleDialectScope;
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::introspection_probe_unchecked::domain::examine;
use crate::support::APPLY_HEADS;

pub const META: RuleMeta = RuleMeta::new(
    "introspection-probe-unchecked",
    // Well-formed code whose meaning is probably not what was intended: the
    // author wrote a lookup and a call, and what happens when the lookup fails
    // is an `undefined-function` error several frames from the real fault.
    RuleCategory::Suspicious,
    Severity::Warning,
    "a lookup that answers nil when not found, applied by funcall/apply with no opportunity to \
     check it",
    // No fix. Whether a missing definition should raise a specific condition,
    // fall back to a default, or be a no-op is a decision about the program.
    Fixability::ReportOnly,
)
.with_explanation(
    RuleExplanation::new(
        "These lookups return nil rather than signalling when the name resolves to nothing. \
         Applying the result in the same expression leaves no point at which the nil could be \
         noticed, so a mistyped name or an unloaded package surfaces as a call to nil.",
    )
    .with_example(
        "(funcall (find-symbol (string-upcase op) :app) request)",
        "(let ((handler (find-symbol (string-upcase op) :app)))\n  (if handler\n      (funcall handler request)\n      (error 'unknown-operation :name op)))",
    )
    .with_caveat(
        "Probing and then checking is the correct idiom and is never reported: binding the result, \
         wrapping it in `or`, or testing it with `when`/`if-let` all put something other than the \
         probe in the function position. Only a probe of a *computed* name is reported — \
         `(funcall (macro-function 'when) …)` names its subject outright.",
    ),
);

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&APPLY_HEADS)
    }

    fn dialect_scope(&self) -> RuleDialectScope {
        RuleDialectScope::new(&[Dialect::CommonLisp, Dialect::EmacsLisp, Dialect::Clojure])
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        if let Some(item) = examine(context.tree(), view, context.dialect()) {
            sink.report(
                item.span,
                format!(
                    "{} applies the result of {} directly; {} answers nil when the name is not \
                     found, so a missing definition becomes a call to nil instead of a checked \
                     branch",
                    item.consumer, item.probe, item.probe
                ),
            );
        }
        Ok(())
    }
}
