//! Loading a project's own rules, and making them indistinguishable from the
//! shipped ones everywhere downstream.
//!
//! Two problems stand between a rule read from a file and the report.
//!
//! **Its name is not `'static`.** [`LintFinding::rule`] is a `&'static str`, so
//! that a consumer outside the domain needs no domain types to read a report,
//! and that is worth keeping. A custom rule's name is a `String` read at
//! startup. Interning it — leaking one boxed string per custom rule, once per
//! process — is what lets a custom finding travel through the summary, the
//! gate, the baseline, the suppression scan, the fix map, SARIF, and the
//! GitHub annotations with no second code path anywhere. The alternative is a
//! parallel finding type threaded through nine output modes.
//!
//! The leak is bounded by the rule *file*, not by the input: a run over ten
//! thousand source files with six custom rules leaks six strings.
//!
//! **Its metadata is not in the registry.** `rule_severity`, `rule_category`,
//! and `rule_is_fixable` read the shipped catalogue and answer "error, no
//! category, not fixable" for a name they do not know — which would report
//! every custom finding at the wrong severity. [`RuleMetaResolver`] is the one
//! place that falls back, and the renderers take it instead of calling the
//! catalogue directly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::application::usecase::lint_report::{
    RULES, Severity, SeverityOverrides, overridden_rule_severity, rule_category, rule_is_fixable,
};
use paredit_feature_lint_custom::{CustomFinding, Ruleset, parse_ruleset, run, run_tests};

/// Where a project keeps its own rules when it does not say otherwise.
pub(super) const DEFAULT_RULE_DIRECTORY: &str = ".paredit/rules";

/// A loaded rule set, with each rule's name interned.
#[derive(Debug, Default)]
pub(super) struct CustomRules {
    ruleset: Ruleset,
    /// `name -> (interned name, category, severity, fixable)`.
    meta: BTreeMap<String, CustomMeta>,
}

#[derive(Debug, Clone, Copy)]
struct CustomMeta {
    name: &'static str,
    category: &'static str,
    severity: Severity,
    fixable: bool,
}

impl CustomRules {
    #[must_use]
    pub(super) fn is_empty(&self) -> bool {
        self.ruleset.is_empty()
    }

    /// The rules, for `--test-rules` and for listing.
    #[must_use]
    pub(super) const fn ruleset(&self) -> &Ruleset {
        &self.ruleset
    }

    /// Whether `rule` names one of these rules.
    #[must_use]
    pub(super) fn is_rule(&self, rule: &str) -> bool {
        self.meta.contains_key(rule)
    }

    /// Every rule's interned name, for widening the active list.
    pub(super) fn names(&self) -> impl Iterator<Item = &'static str> + use<'_> {
        self.meta.values().map(|meta| meta.name)
    }

    /// `(name, category, description, severity, fixable)` for each rule, so
    /// `--list-rules` and `--docs` can present them beside the shipped ones.
    pub(super) fn catalog(
        &self,
    ) -> impl Iterator<Item = (&'static str, &'static str, &str, Severity, bool)> {
        self.ruleset.rules.iter().filter_map(|rule| {
            let meta = self.meta.get(&rule.name)?;
            Some((
                meta.name,
                meta.category,
                rule.description.as_str(),
                meta.severity,
                meta.fixable,
            ))
        })
    }

    /// Every custom finding in one parsed file, as `(rule, span, message,
    /// fix)`, with the rule name interned so it can join a `LintFinding`.
    pub(super) fn findings(
        &self,
        tree: &crate::domain::sexpr::SyntaxTree,
        source: &str,
    ) -> Vec<(&'static str, CustomFinding)> {
        run(&self.ruleset, tree, source)
            .into_iter()
            .filter_map(|finding| {
                let name = self.meta.get(&finding.rule)?.name;
                Some((name, finding))
            })
            .collect()
    }

    /// Runs the `deftest` clauses, returning one line per failure.
    #[must_use]
    pub(super) fn test(&self) -> Vec<String> {
        run_tests(&self.ruleset)
            .into_iter()
            .map(|failure| {
                format!(
                    "{}\t{}\t{}\texpected {}\tgot {}",
                    failure.rule, failure.clause, failure.input, failure.expected, failure.actual
                )
            })
            .collect()
    }
}

/// Loads every `*.lisp` under `directory`, in sorted order.
///
/// Sorted so that two machines reading the same directory produce the same
/// rule order, and therefore the same report order. Directory iteration order
/// is not specified, and a report whose order depends on a filesystem is not
/// reproducible.
fn read_directory(directory: &Path) -> Result<Ruleset> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(directory)
        .with_context(|| format!("reading custom rule directory {}", directory.display()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "lisp")
        })
        .collect();
    paths.sort();

    let mut ruleset = Ruleset::default();
    for path in paths {
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading custom rule file {}", path.display()))?;
        let parsed = parse_ruleset(&path.display().to_string(), &text)
            .with_context(|| format!("in custom rule file {}", path.display()))?;
        ruleset.extend(parsed);
    }
    Ok(ruleset)
}

/// Loads the project's rules.
///
/// With no `--custom-rules`, the default directory is used *if it exists* and
/// the run is otherwise unchanged. A rule file that fails to load fails the
/// run: a project that has written a rule and sees a green build has been told
/// the rule passed.
pub(super) fn load(explicit: Option<&Path>) -> Result<CustomRules> {
    let directory = match explicit {
        Some(path) => path.to_path_buf(),
        None => {
            let default = PathBuf::from(DEFAULT_RULE_DIRECTORY);
            if !default.is_dir() {
                return Ok(CustomRules::default());
            }
            default
        }
    };

    let ruleset = read_directory(&directory)?;
    ruleset
        .validate(&RULES)
        .with_context(|| format!("in custom rules under {}", directory.display()))?;

    let meta = ruleset
        .rules
        .iter()
        .map(|rule| {
            let interned: &'static str = Box::leak(rule.name.clone().into_boxed_str());
            let category: &'static str = rule.category.as_str();
            (
                rule.name.clone(),
                CustomMeta {
                    name: interned,
                    category,
                    severity: rule.severity,
                    fixable: rule.fix.is_some(),
                },
            )
        })
        .collect();

    Ok(CustomRules { ruleset, meta })
}

/// Answers a rule's category, severity, and fixability for *any* rule name,
/// shipped or custom, under the run's `--deny`/`--warn` re-ranking.
///
/// The renderers take this rather than calling the catalogue, which is the one
/// change that makes a custom finding render identically to a shipped one in
/// every output mode.
#[derive(Debug)]
pub(super) struct RuleMetaResolver<'a> {
    overrides: &'a SeverityOverrides,
    custom: &'a CustomRules,
}

impl<'a> RuleMetaResolver<'a> {
    #[must_use]
    pub(super) const fn new(overrides: &'a SeverityOverrides, custom: &'a CustomRules) -> Self {
        Self { overrides, custom }
    }

    #[must_use]
    pub(super) fn severity(&self, rule: &str) -> Severity {
        // A `--deny` naming a custom rule is rejected at argument-parsing time
        // (the selector is checked against the shipped catalogue), so a custom
        // rule's own severity is the answer here.
        self.custom.meta.get(rule).map_or_else(
            || overridden_rule_severity(self.overrides, rule),
            |meta| meta.severity,
        )
    }

    #[must_use]
    pub(super) fn category(&self, rule: &str) -> Option<&'static str> {
        rule_category(rule).or_else(|| self.custom.meta.get(rule).map(|meta| meta.category))
    }

    #[must_use]
    pub(super) fn is_fixable(&self, rule: &str) -> bool {
        self.custom
            .meta
            .get(rule)
            .map_or_else(|| rule_is_fixable(rule), |meta| meta.fixable)
    }

    #[must_use]
    pub(super) const fn overrides(&self) -> &'a SeverityOverrides {
        self.overrides
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a loaded set from source text, without touching the filesystem.
    fn loaded(text: &str) -> CustomRules {
        let ruleset = parse_ruleset("rules.lisp", text).expect("readable");
        let meta = ruleset
            .rules
            .iter()
            .map(|rule| {
                (
                    rule.name.clone(),
                    CustomMeta {
                        name: Box::leak(rule.name.clone().into_boxed_str()),
                        category: rule.category.as_str(),
                        severity: rule.severity,
                        fixable: rule.fix.is_some(),
                    },
                )
            })
            .collect();
        CustomRules { ruleset, meta }
    }

    #[test]
    fn a_custom_rule_resolves_to_its_own_metadata() {
        let custom = loaded(
            r#"(defrule house-style :category naming :severity error
                 :pattern (print ?x) :message "no print" :fix (princ ?x))"#,
        );
        let overrides = SeverityOverrides::new();
        let resolver = RuleMetaResolver::new(&overrides, &custom);

        assert_eq!(resolver.severity("house-style"), Severity::Error);
        assert_eq!(resolver.category("house-style"), Some("naming"));
        assert!(resolver.is_fixable("house-style"));
    }

    #[test]
    fn a_shipped_rule_still_resolves_through_the_catalogue() {
        let custom = CustomRules::default();
        let overrides = SeverityOverrides::new();
        let resolver = RuleMetaResolver::new(&overrides, &custom);

        assert_eq!(resolver.severity("redundant-quote"), Severity::Warning);
        assert_eq!(resolver.category("redundant-quote"), Some("suspicious"));
        assert!(resolver.is_fixable("redundant-quote"));
        assert!(!resolver.is_fixable("if-arity"));
    }

    #[test]
    fn a_deny_still_re_ranks_a_shipped_rule_through_the_resolver() {
        let custom = CustomRules::default();
        let mut overrides = SeverityOverrides::new();
        crate::application::usecase::lint_report::apply_severity_override(
            &mut overrides,
            "redundant-quote",
            Severity::Error,
        )
        .expect("known rule");
        let resolver = RuleMetaResolver::new(&overrides, &custom);
        assert_eq!(resolver.severity("redundant-quote"), Severity::Error);
    }

    #[test]
    fn findings_carry_the_interned_name() {
        let custom = loaded(r#"(defrule no-print :pattern (print ?x) :message "m")"#);
        let source = "(print 1)";
        let tree = crate::domain::sexpr::SyntaxTree::parse(source).expect("parse");
        let found = custom.findings(&tree, source);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "no-print");
        assert_eq!(found[0].1.message, "m");
    }

    #[test]
    fn the_catalog_lists_each_rule_once() {
        let custom = loaded(
            r#"(defrule a :pattern (f) :message "m")
               (defrule b :pattern (g) :message "m" :fix (h))"#,
        );
        let listed: Vec<(&str, bool)> = custom
            .catalog()
            .map(|(name, _, _, _, fixable)| (name, fixable))
            .collect();
        assert_eq!(listed, vec![("a", false), ("b", true)]);
    }

    #[test]
    fn the_harness_reports_one_line_per_failure() {
        let custom = loaded(
            r#"(defrule r :pattern (print ?x) :message "m")
               (deftest r (:matches "(princ 1)"))"#,
        );
        let failures = custom.test();
        assert_eq!(failures.len(), 1);
        assert!(failures[0].starts_with("r\t:matches\t"));
    }

    #[test]
    fn an_empty_set_is_empty() {
        assert!(CustomRules::default().is_empty());
        assert!(CustomRules::default().test().is_empty());
    }
}
