//! The published surface of the lint suite.
//!
//! The rules, the registry they are derived from, and the single pass that
//! runs them live in [`crate::domain::lint`]. This module is the stable façade
//! the application and CLI layers import: the report types, the catalogue
//! constants, and the two entry points that produce findings and fixes for one
//! parsed file.
//!
//! Scope: Common Lisp only, inherited from every underlying rule — each is a
//! documented no-op on other dialects, so the aggregate is too.

use std::path::Path;

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::lint::model::RuleFix;
use crate::domain::lint::policy::RuleSelection;
use crate::domain::lint::registry::catalog;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

pub use crate::domain::lint::model::{
    LintFinding, LintPolicy, LintPolicyOptions, LintSummary, Severity,
};
pub use crate::domain::lint::policy::{
    evaluate_lint_policy, lint_gate_violations, resolve_active_rules, summarize_lint_findings,
};
pub use crate::domain::lint::registry::catalog::{
    CATEGORIES, FIXABLE_RULES, RULE_DOCS, RULES, WARNING_RULES, rule_description, rule_is_fixable,
    rule_severity,
};

/// The category for a rule name, or `None` if the name is unknown.
pub fn rule_category(name: &str) -> Option<&'static str> {
    catalog::rule_category(name).map(|category| category.as_str())
}

/// Runs every rule over one file and returns all findings, tagged by rule.
///
/// Findings come back in the report's canonical order: registry order, then
/// each rule's own pre-order over the document.
pub fn collect_lint_findings(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
) -> Result<Vec<LintFinding>> {
    Ok(crate::domain::lint::collect_lint_outcomes(
        path,
        dialect,
        tree,
        tree.source(),
        RuleSelection::All,
    )?
    .into_iter()
    .map(|outcome| outcome.into_parts().0)
    .collect())
}

/// The automatic rewrite each selected, fixable rule offers for one file,
/// keyed by the finding it repairs.
///
/// Only the `active` rules are considered: an excluded rule must never edit a
/// file, so the selection is applied while fixes are produced rather than
/// afterwards. When a rule reports several findings on the same span the last
/// one wins, matching the map the CLI fix engine has always built.
pub fn collect_lint_fixes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    active: &[&str],
) -> Result<Vec<(&'static str, ByteSpan, RuleFix)>> {
    Ok(crate::domain::lint::collect_lint_outcomes(
        path,
        dialect,
        tree,
        source,
        RuleSelection::Only(active),
    )?
    .into_iter()
    .filter_map(|outcome| {
        let (finding, fix) = outcome.into_parts();
        fix.map(|fix| (finding.rule, finding.span, fix))
    })
    .collect())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn findings(input: &str) -> Vec<LintFinding> {
        let tree = SyntaxTree::parse(input).expect("parse input");
        collect_lint_findings(&PathBuf::from("test.lisp"), Dialect::CommonLisp, &tree)
            .expect("collect lint findings")
    }

    #[test]
    fn rule_docs_cover_every_rule() {
        // Descriptions and categories stay in lockstep with the rule list.
        let documented: Vec<&str> = RULE_DOCS.iter().map(|(rule, _, _)| *rule).collect();
        assert_eq!(documented, RULES.to_vec());
        for rule in RULES {
            assert!(
                rule_description(rule).is_some_and(|doc| !doc.is_empty()),
                "rule {rule} has no description"
            );
            let category = rule_category(rule).expect("rule has a category");
            assert!(
                CATEGORIES.contains(&category),
                "rule {rule} has unknown category {category}"
            );
        }
        assert_eq!(rule_description("no-such-rule"), None);
        assert_eq!(rule_category("no-such-rule"), None);
        // Every fixable rule is a real rule, and the set is non-empty.
        assert!(!FIXABLE_RULES.is_empty());
        for rule in FIXABLE_RULES {
            assert!(RULES.contains(&rule), "fixable rule {rule} is not in RULES");
            assert!(rule_is_fixable(rule));
        }
        assert!(!rule_is_fixable("if-arity"));
        assert!(!rule_is_fixable("no-such-rule"));
        // Every warning rule is a real rule; everything else is an error.
        for rule in WARNING_RULES {
            assert!(RULES.contains(&rule), "warning rule {rule} is not in RULES");
            assert_eq!(rule_severity(rule), Severity::Warning);
        }
        assert_eq!(rule_severity("if-arity"), Severity::Error);
        assert_eq!(rule_severity("literal-place"), Severity::Error);
        // CATEGORIES is sorted and deduplicated, and every category is used.
        let mut sorted = CATEGORIES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, CATEGORIES.to_vec());
        for category in CATEGORIES {
            assert!(
                RULES
                    .iter()
                    .any(|rule| rule_category(rule) == Some(category)),
                "category {category} has no rules"
            );
        }
    }

    #[test]
    fn aggregates_findings_from_multiple_rules() {
        let found = findings("(progn (setq x x) (eql y \"z\"))");
        let rules: Vec<&str> = found.iter().map(|finding| finding.rule).collect();
        assert!(rules.contains(&"self-assignment"));
        assert!(rules.contains(&"eql-string-comparison"));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_clean_file_has_no_findings() {
        let found = findings("(defun f (x y) (+ x y))");
        assert!(found.is_empty());
    }

    #[test]
    fn per_rule_lists_every_active_rule_even_with_zero_findings() {
        let summary = summarize_lint_findings(findings("(setq x x)"), &RULES);
        assert_eq!(summary.per_rule.len(), RULES.len());
        assert_eq!(
            summary
                .per_rule
                .iter()
                .find(|(rule, _)| *rule == "self-assignment")
                .map(|(_, count)| *count),
            Some(1)
        );
        assert_eq!(summary.finding_count, 1);
    }

    #[test]
    fn resolve_active_rules_defaults_to_every_rule() {
        let active = resolve_active_rules(&[], &[], &[]).expect("resolve");
        assert_eq!(active, RULES.to_vec());
    }

    #[test]
    fn resolve_active_rules_honors_only() {
        let active =
            resolve_active_rules(&["self-assignment".to_owned()], &[], &[]).expect("resolve");
        assert_eq!(active, vec!["self-assignment"]);
    }

    #[test]
    fn resolve_active_rules_honors_exclude() {
        let active =
            resolve_active_rules(&[], &["self-assignment".to_owned()], &[]).expect("resolve");
        assert!(!active.contains(&"self-assignment"));
        assert_eq!(active.len(), RULES.len() - 1);
    }

    #[test]
    fn resolve_active_rules_honors_category() {
        let active = resolve_active_rules(&[], &[], &["arity".to_owned()]).expect("resolve");
        assert_eq!(
            active,
            vec![
                "setf-arity",
                "modify-macro-arity",
                "if-arity",
                "the-arity",
                "equality-arity",
                "accessor-arity"
            ]
        );
    }

    #[test]
    fn resolve_active_rules_category_minus_exclude() {
        let active = resolve_active_rules(&[], &["if-arity".to_owned()], &["arity".to_owned()])
            .expect("resolve");
        assert_eq!(
            active,
            vec![
                "setf-arity",
                "modify-macro-arity",
                "the-arity",
                "equality-arity",
                "accessor-arity"
            ]
        );
    }

    #[test]
    fn resolve_active_rules_rejects_an_unknown_rule() {
        assert!(resolve_active_rules(&["not-a-rule".to_owned()], &[], &[]).is_err());
    }

    #[test]
    fn resolve_active_rules_rejects_an_unknown_category() {
        assert!(resolve_active_rules(&[], &[], &["not-a-category".to_owned()]).is_err());
    }

    #[test]
    fn only_selection_filters_findings_to_the_named_rules() {
        // The file has both a self-assignment and an eql-string finding; a
        // `--rule self-assignment` run reports only the former.
        let found = findings("(progn (setq x x) (eql y \"z\"))");
        let summary = summarize_lint_findings(found, &["self-assignment"]);
        assert_eq!(summary.finding_count, 1);
        assert_eq!(summary.per_rule, vec![("self-assignment", 1)]);
    }

    #[test]
    fn ignores_non_common_lisp_dialects() {
        let tree = SyntaxTree::parse("(setq x x)").expect("parse input");
        let found = collect_lint_findings(&PathBuf::from("app.clj"), Dialect::Clojure, &tree)
            .expect("collect lint findings");
        assert!(found.is_empty());
    }

    #[test]
    fn policy_fails_only_when_flag_set() {
        let summary = summarize_lint_findings(findings("(setq x x)"), &RULES);

        let quiet = evaluate_lint_policy(LintPolicyOptions::new(false, None), &summary);
        assert!(quiet.passed);
        assert_eq!(quiet.finding_count, 1);

        let strict = evaluate_lint_policy(LintPolicyOptions::new(true, None), &summary);
        assert!(!strict.passed);
    }

    #[test]
    fn severity_gate_fails_only_at_or_above_the_threshold() {
        // self-assignment is an error; redundant-quote is a warning.
        let error_only = summarize_lint_findings(findings("(setq x x)"), &RULES);
        let warning_only = summarize_lint_findings(findings("(list '5)"), &RULES);

        // --fail-on error trips on the error, not on the warning.
        let opts = LintPolicyOptions::new(false, Some(Severity::Error));
        assert!(!evaluate_lint_policy(opts, &error_only).passed);
        assert!(evaluate_lint_policy(opts, &warning_only).passed);

        // --fail-on warning trips on either (warning is the lower bar).
        let opts = LintPolicyOptions::new(false, Some(Severity::Warning));
        assert!(!evaluate_lint_policy(opts, &error_only).passed);
        assert!(!evaluate_lint_policy(opts, &warning_only).passed);
    }
}
