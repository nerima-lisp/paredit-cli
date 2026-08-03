//! `unused-local-binding`: registers the detection in
//! [`crate::unused_local_binding::domain`] with the lint suite.

use paredit_core_lint_engine::LintResult;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

use crate::unused_local_binding::domain::examine;

pub const META: RuleMeta = RuleMeta::new(
    "unused-local-binding",
    RuleCategory::DeadCode,
    Severity::Warning,
    "a let/let*/flet/labels binding that nothing in its body reads",
    // Not fixable. Deleting a binding deletes its initial form with it, and
    // that form may be there for its side effect: `(let ((x (pop queue))) …)`
    // that never reads `x` still has to pop. The repair is a judgement call
    // between deleting the binding, keeping the call and dropping the name,
    // and declaring it ignored, and nothing here can tell which.
    Fixability::ReportOnly,
);

/// Exactly the four heads [`crate::unused_local_binding::domain::binder_kind`]
/// answers for.
///
/// `Heads` rather than `WholeTree`: the rule is a per-form check, and the
/// benchmark gate that a whole-tree pass trips has failed this project five
/// times.
const HEADS: [NormalizedHead; 4] = [
    NormalizedHead::new("let"),
    NormalizedHead::new("let*"),
    NormalizedHead::new("flet"),
    NormalizedHead::new("labels"),
];

#[derive(Debug)]
pub struct Rule;

pub const RULE: Rule = Rule;

impl LintRule for Rule {
    fn head_filter(&self) -> HeadFilter {
        HeadFilter::Heads(&HEADS)
    }

    fn check(
        &self,
        context: &RuleContext<'_>,
        view: &ExpressionView,
        sink: &mut RuleSink<'_, '_>,
    ) -> LintResult<()> {
        for finding in examine(context, view, true).findings {
            sink.report(finding.span, finding.message());
        }
        Ok(())
    }
}
