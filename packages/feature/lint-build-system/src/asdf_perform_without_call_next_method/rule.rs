//! `asdf-perform-without-call-next-method`: a primary `perform` method that
//! replaces ASDF's standard build step instead of extending it.
//!
//!
//! Anchored on `defmethod`, not on `perform`: `perform` is the *name* of the
//! generic function, which sits at child 1, and the engine's head index is keyed
//! on child 0. The rule re-checks the name itself before doing anything else.

use paredit_core_lint_engine::LintResult;

use crate::asdf_perform_without_call_next_method::domain::examine_defmethod;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "asdf-perform-without-call-next-method",
    // The defect is in how the method participates in the standard method
    // combination — a primary method shadowing the one it meant to extend.
    // That is CLOS, which is what `ObjectSystem` covers; the fact that the
    // generic function happens to be ASDF's decides the rule's package, not its
    // category.
    RuleCategory::ObjectSystem,
    // Fully replacing the standard method is a legitimate, if drastic, choice,
    // so this is an inference about intent rather than a settled defect.
    Severity::Warning,
    "a primary asdf perform method on load-op or compile-op that never calls call-next-method",
    // Where the call belongs, and whether its value should be returned, is a
    // human decision.
    Fixability::ReportOnly,
);

/// `examine_defmethod` only ever matches a `defmethod` head; the generic
/// function's name is checked inside it.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defmethod")];

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
        let mut perform_method_count = 0;
        let mut items = Vec::new();
        examine_defmethod(view, &mut perform_method_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only now: a `(defmethod …)` inside `'(…)` is a list of symbols.
        // Dispatch cannot tell, and asking costs a descent, so the question is
        // asked once a finding already exists rather than once per node.
        if is_unevaluated_at(context.tree(), view.span) {
            return Ok(());
        }
        for item in items {
            sink.report(item.span, item.message());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::run_rule;
    use paredit_core_lint_engine::rule::RuleEntry;

    /// A one-rule catalogue, so the engine's head index — the thing that
    /// decides whether `check` is ever called — is exercised for real.
    static ENTRIES: [RuleEntry; 1] = [RuleEntry::new(&META, &RULE)];

    fn messages(source: &str) -> Vec<String> {
        run_rule(&ENTRIES, source)
    }

    #[test]
    fn the_head_filter_reaches_defmethod_and_the_name_is_rechecked() {
        assert_eq!(
            messages("(defmethod perform ((o load-op) (c cl-source-file)) (boot c))").len(),
            1
        );
        assert_eq!(
            messages("(cl:defmethod asdf:perform ((o load-op) (c cl-source-file)) (boot c))").len(),
            1
        );
        // Same head, different generic function.
        assert!(
            messages("(defmethod output-files ((o load-op) (c cl-source-file)) nil)").is_empty()
        );
    }

    #[test]
    fn a_composing_method_produces_nothing_through_the_engine() {
        assert!(
            messages("(defmethod perform ((o load-op) (c cl-source-file)) (call-next-method))")
                .is_empty()
        );
    }

    #[test]
    fn a_quoted_defmethod_produces_nothing_through_the_engine() {
        let offender = "(defmethod perform ((o load-op) (c cl-source-file)) (boot c))";
        assert!(messages(&format!("'{offender}")).is_empty());
        assert!(messages(&format!("(quote {offender})")).is_empty());
        assert!(messages(&format!("'(a ,{offender})")).is_empty());
        assert!(messages(&format!("`{offender}")).is_empty());
        assert_eq!(messages(&format!("`(a ,{offender})")).len(), 1);
    }
}
