//! The published surface of the lint suite.
//!
//! The rules, the registry they are derived from, and the single pass that
//! runs them live in `crate::domain::lint`. This module is the stable façade
//! the application and CLI layers import: the report types, the catalogue
//! constants, and the two entry points that produce findings and fixes for one
//! parsed file.
//!
//! Scope: Common Lisp only, inherited from every underlying rule — each is a
//! documented no-op on other dialects, so the aggregate is too.

use std::path::Path;

use anyhow::Result;

use crate::domain::dialect::Dialect;
use crate::domain::lint::engine::PassOptions;
pub use crate::domain::lint::engine::RuleTimings;
pub use crate::domain::lint::model::RuleFix;
use crate::domain::lint::policy::RuleSelection;
use crate::domain::lint::registry::catalog;
use crate::domain::sexpr::{ByteSpan, SyntaxTree};

pub use crate::domain::lint::model::{
    FindingId, LintFinding, LintPolicy, LintPolicyOptions, LintSummary, RuleExample,
    RuleExplanation, RuleSetting, RuleSettings, RuleTag, RuleTags, Severity,
};
pub use crate::domain::lint::policy::{RuleFilter, RulePreset, SeverityOverrides};
// The registry-injecting wrappers, not the raw `policy` functions: those now
// take the catalogue explicitly so the engine can live without a registry.
pub use crate::domain::lint::registry::catalog::{
    CATEGORIES, EXPERIMENTAL_RULES, FIXABLE_RULES, PEDANTIC_RULES, RULE_DOCS, RULES, TAGS,
    WARNING_RULES, rule_description, rule_dialects, rule_explanation, rule_is_fixable,
    rule_setting, rule_settings, rule_severity, rule_tags,
};
pub use crate::domain::lint::{
    apply_severity_override, evaluate_lint_policy, lint_gate_violations, overridden_rule_severity,
    resolve_active_rules, summarize_lint_findings,
};

/// One rule's rewrite for one finding: the rule that offered it and the span
/// of the finding it repairs, which together key the fix map its callers
/// build.
pub type RuleFixFor = (&'static str, ByteSpan, RuleFix);

/// The category for a rule name, or `None` if the name is unknown.
#[must_use]
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
) -> Result<Vec<RuleFixFor>> {
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

/// Every finding in a file, together with the fixes the `active` rules offer,
/// from a single dispatch pass.
///
/// `--sarif` and `--fix-plan` need both halves of the same walk: the complete
/// finding list, so the report covers every rule, and the fixes of the
/// selected rules only, so an excluded rule never edits a file. Asking
/// [`collect_lint_findings`] and [`collect_lint_fixes`] for them separately
/// ran that pass twice over every file — `--fix` has always managed with one.
///
/// Filtering after the walk is identical to filtering before it, and that is
/// not an accident of the current rules:
/// [`crate::domain::lint::policy::RuleSelection`] is a per-rule
/// membership test, rules never observe one another, and a rule left out of
/// `active` contributes no fix either way. The one thing that must not happen
/// — a fix from an unselected rule reaching the plan — is exactly what the
/// `active` check below prevents.
pub fn collect_lint_findings_and_fixes(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    active: &[&str],
) -> Result<(Vec<LintFinding>, Vec<RuleFixFor>)> {
    let outcomes = crate::domain::lint::collect_lint_outcomes(
        path,
        dialect,
        tree,
        source,
        RuleSelection::All,
    )?;

    let mut findings = Vec::with_capacity(outcomes.len());
    let mut fixes = Vec::new();
    for outcome in outcomes {
        let (finding, fix) = outcome.into_parts();
        if let Some(fix) = fix.filter(|_| active.contains(&finding.rule)) {
            fixes.push((finding.rule, finding.span, fix));
        }
        findings.push(finding);
    }
    Ok((findings, fixes))
}

/// What one file's pass should compute beyond the findings themselves.
#[derive(Debug, Clone, Copy, Default)]
pub struct LintPassRequest<'a> {
    /// The rules allowed to contribute a *fix*. Findings are always collected
    /// for every rule, because a report covers the whole suite and filters
    /// afterwards; a fix from an unselected rule must never reach a file.
    pub active: &'a [&'a str],
    /// The caller's `--rule-arg` overrides.
    pub settings: Option<&'a RuleSettings>,
    /// Whether to time each rule (`--timings`).
    pub measure: bool,
}

/// One file's pass: every finding, the selected rules' fixes, and — when asked
/// for — what each rule cost.
#[derive(Debug)]
pub struct LintPassResult {
    pub findings: Vec<LintFinding>,
    pub fixes: Vec<RuleFixFor>,
    pub timings: Option<RuleTimings>,
}

/// Runs the suite over one file and returns everything a report can need from
/// a single walk.
///
/// The three narrower collectors above predate the per-run knobs and remain
/// because they read better at their call sites; each is this function with
/// two of the three outputs discarded. Every output path that needs more than
/// one of them goes through here, so no path walks a file twice.
pub fn run_lint_pass(
    path: &Path,
    dialect: Dialect,
    tree: &SyntaxTree,
    source: &str,
    request: LintPassRequest<'_>,
) -> Result<LintPassResult> {
    let outcome = crate::domain::lint::collect_lint_pass(
        path,
        dialect,
        tree,
        source,
        RuleSelection::All,
        PassOptions {
            settings: request.settings,
            measure: request.measure,
        },
    )?;

    let mut findings = Vec::with_capacity(outcome.outcomes.len());
    let mut fixes = Vec::new();
    for item in outcome.outcomes {
        let (finding, fix) = item.into_parts();
        if let Some(fix) = fix.filter(|_| request.active.contains(&finding.rule)) {
            fixes.push((finding.rule, finding.span, fix));
        }
        findings.push(finding);
    }
    Ok(LintPassResult {
        findings,
        fixes,
        timings: outcome.timings,
    })
}

/// Pairs a timing table with the rule names it is indexed by.
///
/// [`RuleTimings`] is indexed by registration position and never names a rule
/// — the engine has no registry. `RULES` is derived from the same registry in
/// the same order, so the join is an index, and doing it here keeps the
/// invariant "these two arrays agree" in the module that owns both.
#[must_use]
pub fn rule_timing_report(timings: &RuleTimings) -> Vec<(&'static str, std::time::Duration, u64)> {
    timings
        .entries()
        .map(|(position, elapsed, invocations)| (RULES[position], elapsed, invocations))
        .collect()
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
    fn every_tagged_rule_is_a_real_rule() {
        for rule in EXPERIMENTAL_RULES {
            assert!(RULES.contains(&rule), "{rule} is not in RULES");
            assert!(rule_tags(rule).contains(RuleTag::Experimental));
        }
        for rule in PEDANTIC_RULES {
            assert!(RULES.contains(&rule), "{rule} is not in RULES");
            assert!(rule_tags(rule).contains(RuleTag::Pedantic));
        }
    }

    #[test]
    fn a_rule_that_declares_a_setting_declares_it_completely() {
        for rule in RULES {
            let mut keys = Vec::new();
            for setting in rule_settings(rule) {
                assert!(
                    !setting.description().is_empty(),
                    "{rule}.{} has no description",
                    setting.key()
                );
                assert!(
                    rule_setting(rule, setting.key()).is_some(),
                    "{rule}.{} is not findable by key",
                    setting.key()
                );
                keys.push(setting.key());
            }
            let unique: std::collections::BTreeSet<&str> = keys.iter().copied().collect();
            assert_eq!(unique.len(), keys.len(), "{rule} declares a key twice");
        }
    }

    #[test]
    fn every_rule_reports_at_least_one_dialect() {
        // A rule scoped to no dialect can never fire, which is a registration
        // mistake no other test would notice.
        for rule in RULES {
            assert!(
                !rule_dialects(rule).is_empty(),
                "{rule} is scoped to no dialect"
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
        // Documented, because `missing-docstring` is one of the rules being
        // run: this collector applies every rule, and the preset that would
        // normally hold that one back is a *selection* concern above it.
        let found = findings(r#"(defun f (x y) "Add X and Y." (+ x y))"#);
        assert!(found.is_empty(), "unexpected findings: {found:?}");
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

    /// The historical three selectors, which is what most of these tests vary.
    fn resolve(only: &[String], exclude: &[String], categories: &[String]) -> Vec<&'static str> {
        resolve_active_rules(&RuleFilter::named(only, exclude, categories)).expect("resolve")
    }

    /// Every rule the default preset admits — which is every shipped rule that
    /// carries neither the `pedantic` nor the `experimental` tag.
    fn recommended() -> Vec<&'static str> {
        RULES
            .iter()
            .copied()
            .filter(|rule| {
                !rule_tags(rule).contains(RuleTag::Pedantic)
                    && !rule_tags(rule).contains(RuleTag::Experimental)
            })
            .collect()
    }

    #[test]
    fn resolve_active_rules_defaults_to_the_recommended_preset() {
        assert_eq!(resolve(&[], &[], &[]), recommended());
    }

    #[test]
    fn the_all_preset_admits_every_shipped_rule() {
        let filter = RuleFilter {
            preset: RulePreset::All,
            ..RuleFilter::default()
        };
        assert_eq!(
            resolve_active_rules(&filter).expect("resolve"),
            RULES.to_vec()
        );
    }

    #[test]
    fn resolve_active_rules_honors_only() {
        let active = resolve(&["self-assignment".to_owned()], &[], &[]);
        assert_eq!(active, vec!["self-assignment"]);
    }

    #[test]
    fn resolve_active_rules_honors_exclude() {
        let active = resolve(&[], &["self-assignment".to_owned()], &[]);
        assert!(!active.contains(&"self-assignment"));
        assert_eq!(active.len(), recommended().len() - 1);
    }

    #[test]
    fn resolve_active_rules_honors_category() {
        let active = resolve(&[], &[], &["arity".to_owned()]);
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
        let active = resolve(&[], &["if-arity".to_owned()], &["arity".to_owned()]);
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
        let only = ["not-a-rule".to_owned()];
        assert!(resolve_active_rules(&RuleFilter::named(&only, &[], &[])).is_err());
    }

    #[test]
    fn resolve_active_rules_rejects_an_unknown_category() {
        let categories = ["not-a-category".to_owned()];
        assert!(resolve_active_rules(&RuleFilter::named(&[], &[], &categories)).is_err());
    }

    #[test]
    fn the_minimal_preset_is_the_error_rules_of_the_recommended_one() {
        let filter = RuleFilter {
            preset: RulePreset::Minimal,
            ..RuleFilter::default()
        };
        let minimal = resolve_active_rules(&filter).expect("resolve");
        let expected: Vec<&str> = recommended()
            .into_iter()
            .filter(|rule| rule_severity(rule) == Severity::Error)
            .collect();
        assert_eq!(minimal, expected);
        assert!(!minimal.is_empty());
        assert!(minimal.len() < recommended().len());
    }

    #[test]
    fn severity_overrides_promote_and_demote_against_the_shipped_catalogue() {
        let mut overrides = SeverityOverrides::new();
        // redundant-quote ships as a warning; if-arity ships as an error.
        assert_eq!(
            overridden_rule_severity(&overrides, "redundant-quote"),
            Severity::Warning
        );
        apply_severity_override(&mut overrides, "redundant-quote", Severity::Error)
            .expect("known rule");
        apply_severity_override(&mut overrides, "if-arity", Severity::Warning).expect("known rule");
        assert_eq!(
            overridden_rule_severity(&overrides, "redundant-quote"),
            Severity::Error
        );
        assert_eq!(
            overridden_rule_severity(&overrides, "if-arity"),
            Severity::Warning
        );
        assert!(apply_severity_override(&mut overrides, "no-such-rule", Severity::Error).is_err());
    }

    #[test]
    fn a_category_severity_override_reaches_every_rule_in_it() {
        let mut overrides = SeverityOverrides::new();
        apply_severity_override(&mut overrides, "arity", Severity::Warning)
            .expect("known category");
        for rule in RULES {
            if rule_category(rule) == Some("arity") {
                assert_eq!(
                    overridden_rule_severity(&overrides, rule),
                    Severity::Warning,
                    "{rule} was not demoted"
                );
            }
        }
    }

    #[test]
    fn one_pass_answers_findings_fixes_and_timings_together() {
        let source = "(progn (setq x x) (the t y))";
        let tree = SyntaxTree::parse(source).expect("parse input");
        let active = RULES.to_vec();
        let result = run_lint_pass(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            LintPassRequest {
                active: &active,
                settings: None,
                measure: true,
            },
        )
        .expect("run pass");

        let rules: Vec<&str> = result.findings.iter().map(|f| f.rule).collect();
        assert!(rules.contains(&"self-assignment"));
        assert!(rules.contains(&"redundant-the"));
        // self-assignment is report-only; redundant-the is fixable.
        let fixed: Vec<&str> = result.fixes.iter().map(|(rule, _, _)| *rule).collect();
        assert_eq!(fixed, vec!["redundant-the"]);

        let timings = result.timings.expect("--timings was requested");
        let report = rule_timing_report(&timings);
        assert_eq!(report.len(), RULES.len());
        assert_eq!(report[0].0, RULES[0]);
        // Every rule the pass ran was invoked at least once on this file.
        assert!(
            report
                .iter()
                .any(|(rule, _, invocations)| *rule == "redundant-the" && *invocations > 0)
        );
    }

    #[test]
    fn a_pass_that_was_not_asked_to_measure_reports_no_timings() {
        let source = "(setq x x)";
        let tree = SyntaxTree::parse(source).expect("parse input");
        let result = run_lint_pass(
            &PathBuf::from("test.lisp"),
            Dialect::CommonLisp,
            &tree,
            source,
            LintPassRequest::default(),
        )
        .expect("run pass");
        assert!(result.timings.is_none());
        // No rule was `active`, so no fix is offered even for a fixable rule.
        assert!(result.fixes.is_empty());
        assert_eq!(result.findings.len(), 1);
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

        let quiet = evaluate_lint_policy(
            &SeverityOverrides::new(),
            LintPolicyOptions::new(false, None),
            &summary,
        );
        assert!(quiet.passed);
        assert_eq!(quiet.finding_count, 1);

        let strict = evaluate_lint_policy(
            &SeverityOverrides::new(),
            LintPolicyOptions::new(true, None),
            &summary,
        );
        assert!(!strict.passed);
    }

    #[test]
    fn a_denied_rule_trips_a_gate_its_shipped_severity_would_not() {
        // redundant-quote ships as a warning, so `--fail-on error` passes...
        let summary = summarize_lint_findings(findings("(list '5)"), &RULES);
        let options = LintPolicyOptions::new(false, Some(Severity::Error));
        assert!(evaluate_lint_policy(&SeverityOverrides::new(), options, &summary).passed);

        // ...until the run says otherwise. A `--deny` that changed only the
        // printed severity would have changed nothing that matters.
        let mut overrides = SeverityOverrides::new();
        apply_severity_override(&mut overrides, "redundant-quote", Severity::Error)
            .expect("known rule");
        assert!(!evaluate_lint_policy(&overrides, options, &summary).passed);
    }

    #[test]
    fn a_warned_rule_stops_tripping_a_gate_its_shipped_severity_would() {
        let summary = summarize_lint_findings(findings("(incf 5)"), &RULES);
        let options = LintPolicyOptions::new(false, Some(Severity::Error));
        assert!(!evaluate_lint_policy(&SeverityOverrides::new(), options, &summary).passed);

        let mut overrides = SeverityOverrides::new();
        apply_severity_override(&mut overrides, "literal-place", Severity::Warning)
            .expect("known rule");
        assert!(evaluate_lint_policy(&overrides, options, &summary).passed);
    }

    #[test]
    fn severity_gate_fails_only_at_or_above_the_threshold() {
        // self-assignment is an error; redundant-quote is a warning.
        let error_only = summarize_lint_findings(findings("(setq x x)"), &RULES);
        let warning_only = summarize_lint_findings(findings("(list '5)"), &RULES);

        // --fail-on error trips on the error, not on the warning.
        let opts = LintPolicyOptions::new(false, Some(Severity::Error));
        assert!(!evaluate_lint_policy(&SeverityOverrides::new(), opts, &error_only).passed);
        assert!(evaluate_lint_policy(&SeverityOverrides::new(), opts, &warning_only).passed);

        // --fail-on warning trips on either (warning is the lower bar).
        let opts = LintPolicyOptions::new(false, Some(Severity::Warning));
        assert!(!evaluate_lint_policy(&SeverityOverrides::new(), opts, &error_only).passed);
        assert!(!evaluate_lint_policy(&SeverityOverrides::new(), opts, &warning_only).passed);
    }
}
