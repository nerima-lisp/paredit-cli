//! `asdf-self-referential-depends-on`: a system whose `:depends-on` names
//! itself.
//!

use paredit_core_lint_engine::LintResult;

use crate::asdf_self_referential_depends_on::domain::examine_defsystem;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "asdf-self-referential-depends-on",
    // Not `Duplicate` (nothing is written twice) and not `Malformed` (the form
    // is exactly the shape ASDF requires). The defect is that a well-formed
    // declaration says something that cannot be true, which is what
    // `Suspicious` is for.
    RuleCategory::Suspicious,
    // ASDF stops the build on this; there is no reading of the form under which
    // it is what the author wanted.
    Severity::Error,
    "a defsystem whose :depends-on names the system itself",
    // Delete the entry or rename it to the system that was meant — a decision,
    // not a rewrite.
    Fixability::ReportOnly,
);

/// `examine_defsystem` only ever matches a `defsystem` head. The engine's head
/// index folds `asdf:defsystem`, `asdf::defsystem` and
/// `asdf/parse-defsystem:defsystem` onto this same key.
const HEADS: [NormalizedHead; 1] = [NormalizedHead::new("defsystem")];

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
        let mut dependency_count = 0;
        let mut items = Vec::new();
        examine_defsystem(view, &mut dependency_count, &mut items);
        if items.is_empty() {
            return Ok(());
        }
        // Only now: a `(defsystem …)` inside `'(…)` is a list of symbols.
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
    fn the_head_filter_reaches_every_spelling_of_defsystem() {
        for head in [
            "defsystem",
            "asdf:defsystem",
            "asdf/parse-defsystem:defsystem",
        ] {
            assert_eq!(
                messages(&format!("({head} \"app\" :depends-on (\"app\"))")).len(),
                1,
                "the head index did not dispatch `{head}` to this rule"
            );
        }
    }

    #[test]
    fn a_dependency_on_another_system_produces_nothing_through_the_engine() {
        assert!(messages("(defsystem \"app\" :depends-on (\"alexandria\"))").is_empty());
    }

    #[test]
    fn a_quoted_defsystem_produces_nothing_through_the_engine() {
        let offender = "(defsystem \"app\" :depends-on (\"app\"))";
        assert!(messages(&format!("'{offender}")).is_empty());
        assert!(messages(&format!("(quote {offender})")).is_empty());
        assert!(messages(&format!("'(a ,{offender})")).is_empty());
        assert!(messages(&format!("`{offender}")).is_empty());
        assert_eq!(messages(&format!("`(a ,{offender})")).len(), 1);
    }
}
