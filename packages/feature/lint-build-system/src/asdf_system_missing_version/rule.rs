//! `asdf-system-missing-version`: a primary ASDF system with no `:version`.
//!
//! The analysis lives in [`crate::asdf_system_missing_version::domain`], which
//! also backs the standalone `inspect asdf-system-missing-version` command;
//! this module only registers it with the lint suite and phrases its findings.

use paredit_core_lint_engine::LintResult;

use crate::asdf_system_missing_version::domain::examine_defsystem;
use crate::support::is_unevaluated_at;
use paredit_core_cli::report::Finding;
use paredit_core_lint_engine::engine::{RuleContext, RuleSink};
use paredit_core_lint_engine::model::{
    Fixability, HeadFilter, NormalizedHead, RuleCategory, RuleMeta, RuleTag, Severity,
};
use paredit_core_lint_engine::rule::LintRule;
use paredit_core_syntax::sexpr::ExpressionView;

pub const META: RuleMeta = RuleMeta::new(
    "asdf-system-missing-version",
    // The `:version` is declared metadata about the system — what
    // `asdf:component-version` publishes and what a dependant's version floor
    // reads. Its absence is a gap in what the system says about itself, not a
    // malformed form (`:version` is optional) and not a wrong-looking one.
    RuleCategory::Documentation,
    // A system without a version loads and builds. What breaks is what other
    // code can say *about* it, which is a real but not build-stopping defect.
    Severity::Warning,
    "a released defsystem with no :version option",
    // What the version should be is the one thing a rewrite cannot know.
    Fixability::ReportOnly,
)
// Measured, not assumed: over a local Quicklisp checkout this rule fires on
// roughly 10% of primary systems even after the support-system exemption, and
// every one of those is correct code by a maintainer who omitted `:version` on
// purpose (`babel`, `cffi` and its subsystems, `trivial-garbage`, …). That is a
// convention worth offering and not one to assert by default, which is exactly
// what `Pedantic` encodes: `recommended` and `minimal` exclude it, `pedantic`
// turns it on. See this rule's `domain` module docs for the survey.
.with_tags(&[RuleTag::Pedantic]);

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
        let mut primary_system_count = 0;
        let mut items = Vec::new();
        examine_defsystem(view, &mut primary_system_count, &mut items);
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
            "asdf::defsystem",
            "asdf/parse-defsystem:defsystem",
            "ASDF:DEFSYSTEM",
        ] {
            assert_eq!(
                messages(&format!("({head} \"app\")")).len(),
                1,
                "the head index did not dispatch `{head}` to this rule"
            );
        }
    }

    #[test]
    fn a_versioned_system_produces_nothing_through_the_engine() {
        assert!(messages("(defsystem \"app\" :version \"1.0\")").is_empty());
    }

    #[test]
    fn a_quoted_defsystem_produces_nothing_through_the_engine() {
        // The engine's walk descends into quoted data, so this is the path
        // `is_unevaluated_at` exists for.
        assert!(messages("'(defsystem \"app\")").is_empty());
        assert!(messages("(quote (defsystem \"app\"))").is_empty());
        assert!(messages("'(a ,(defsystem \"app\"))").is_empty());
        assert!(messages("`(defsystem \"app\")").is_empty());
        assert_eq!(messages("`(a ,(defsystem \"app\"))").len(), 1);
    }

    #[test]
    fn the_rule_is_tagged_pedantic_so_the_default_presets_exclude_it() {
        assert!(META.has_tag(RuleTag::Pedantic));
    }
}
