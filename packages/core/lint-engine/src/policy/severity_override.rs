//! Per-run promotion and demotion of a rule's severity.
//!
//! A rule's shipped [`Severity`] is the suite's opinion, and the suite is not
//! the one whose build breaks. A project that has decided `redundant-quote` is
//! non-negotiable wants it to fail CI; a project mid-migration wants
//! `if-arity` reported without failing anything. Today both have to reach for
//! `--exclude`, which throws the finding away rather than re-ranking it.
//!
//! So severity becomes a *run* property layered over the shipped one. The
//! override is keyed by rule or by category, resolved rule-first so
//! `--warn suspicious --deny redundant-quote` reads the way it looks.
//!
//! Disabling stays out of scope here on purpose: `--allow` feeds the existing
//! `exclude` list, so "not reported at all" and "reported at another level"
//! remain visibly different mechanisms.

use std::collections::BTreeMap;

use crate::error::{RuleSelectionError, did_you_mean};
use crate::model::Severity;
use crate::rule::RuleCatalog;

/// The severity each rule is reported at for one run.
///
/// Empty for a run with no `--deny`/`--warn`, which is almost all of them; an
/// empty map answers every query from the catalogue with no allocation.
#[derive(Debug, Clone, Default)]
pub struct SeverityOverrides {
    by_rule: BTreeMap<&'static str, Severity>,
}

impl SeverityOverrides {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies one `--deny`/`--warn` selector, which names either a rule, a
    /// category, or — when named in `custom_rules` — a project's own loaded
    /// `defrule` rule.
    ///
    /// A name that is none of those is a hard error rather than a silent
    /// no-op: the whole point of the flag is to change what fails CI, and a
    /// typo that quietly changed nothing would leave a build passing for the
    /// wrong reason.
    ///
    /// A custom rule has no category to re-rank alongside it — `matched`
    /// tracks only the catalogue's categories — so a custom name is checked
    /// once its own name has already missed both the catalogue's rules and
    /// categories.
    pub fn apply(
        &mut self,
        catalog: RuleCatalog,
        selector: &str,
        severity: Severity,
        custom_rules: &[&'static str],
    ) -> Result<(), RuleSelectionError> {
        let mut matched = false;
        for entry in catalog.entries() {
            let meta = entry.meta();
            if meta.name().as_str() == selector {
                self.by_rule.insert(meta.name().as_str(), severity);
                return Ok(());
            }
            if meta.category().as_str() == selector {
                self.by_rule.insert(meta.name().as_str(), severity);
                matched = true;
            }
        }
        if matched {
            return Ok(());
        }
        if let Some(name) = custom_rules.iter().copied().find(|name| *name == selector) {
            self.by_rule.insert(name, severity);
            return Ok(());
        }
        let known: Vec<&str> = catalog
            .names()
            .chain(catalog.categories())
            .chain(custom_rules.iter().copied())
            .collect();
        Err(RuleSelectionError::UnknownRule {
            suggestion: did_you_mean(known.iter().copied(), selector),
            name: selector.to_owned(),
            valid: known.join(", "),
        })
    }

    /// The severity `rule` is reported at, falling back to its shipped one.
    #[must_use]
    pub fn severity_of(&self, catalog: RuleCatalog, rule: &str) -> Severity {
        self.by_rule
            .get(rule)
            .copied()
            .unwrap_or_else(|| catalog.severity_of(rule))
    }

    /// The severity specifically recorded for `rule` by `--deny`/`--warn`, with
    /// no catalogue fallback.
    ///
    /// Distinct from [`Self::severity_of`], which always answers something: a
    /// custom rule has no catalogue entry to fall back to, so its own resolver
    /// needs to ask "was this specifically overridden?" before reaching for
    /// the rule's own declared severity. (A rule [`Self::seed`] has filled in
    /// answers `Some` here too — from that call's point of view a seeded
    /// default *is* the rule's recorded severity, which is exactly what makes
    /// [`Self::severity_of`] correct for it without a catalogue entry to fall
    /// back to.)
    #[must_use]
    pub fn override_of(&self, rule: &str) -> Option<Severity> {
        self.by_rule.get(rule).copied()
    }

    /// Records `rule`'s severity if — and only if — `--deny`/`--warn` has not
    /// already recorded one for it.
    ///
    /// [`Self::severity_of`] falls back to [`RuleCatalog::severity_of`] for a
    /// rule this map does not mention, and that fallback answers `Error` for
    /// *any* name the catalogue does not register — the correct read for a
    /// genuine typo, but the wrong one for a project's own loaded rule, which
    /// has a real severity of its own that simply is not in this catalogue.
    /// Seeding that severity here, before any `--deny`/`--warn` is applied,
    /// gives [`Self::severity_of`] (and therefore the CI gate) the right
    /// answer without a custom rule ever having to fake a catalogue entry —
    /// and an explicit `--deny`/`--warn` naming that same rule still wins,
    /// since [`Self::apply`] inserts unconditionally over whatever is here.
    pub fn seed(&mut self, rule: &'static str, severity: Severity) {
        self.by_rule.entry(rule).or_insert(severity);
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_rule.is_empty()
    }

    /// Every override as `(rule, severity)`, sorted, so a report can state what
    /// a run actually applied.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, Severity)> {
        self.by_rule
            .iter()
            .map(|(rule, severity)| (*rule, *severity))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fixability, RuleCategory, RuleMeta};
    use crate::rule::{LintRule, RuleEntry};

    #[derive(Debug)]
    struct Noop;

    impl LintRule for Noop {
        fn head_filter(&self) -> crate::model::HeadFilter {
            crate::model::HeadFilter::AllNodes
        }

        fn check(
            &self,
            _: &crate::engine::RuleContext<'_>,
            _: &paredit_core_syntax::sexpr::ExpressionView,
            _: &mut crate::engine::RuleSink<'_, '_>,
        ) -> crate::error::LintResult {
            Ok(())
        }
    }

    static NOOP: Noop = Noop;

    static QUOTE: RuleMeta = RuleMeta::new(
        "redundant-quote",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a quote that quotes a self-evaluating object",
        Fixability::Fixable,
    );
    static ARITY: RuleMeta = RuleMeta::new(
        "if-arity",
        RuleCategory::Arity,
        Severity::Error,
        "an if with too many branches",
        Fixability::ReportOnly,
    );
    static NESTED: RuleMeta = RuleMeta::new(
        "nested-progn",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a progn directly inside a progn",
        Fixability::Fixable,
    );

    static ENTRIES: [RuleEntry; 3] = [
        RuleEntry::new(&QUOTE, &NOOP),
        RuleEntry::new(&ARITY, &NOOP),
        RuleEntry::new(&NESTED, &NOOP),
    ];

    fn catalog() -> RuleCatalog {
        RuleCatalog::new(&ENTRIES)
    }

    #[test]
    fn with_no_override_the_shipped_severity_stands() {
        let overrides = SeverityOverrides::new();
        assert!(overrides.is_empty());
        assert_eq!(
            overrides.severity_of(catalog(), "redundant-quote"),
            Severity::Warning
        );
        assert_eq!(
            overrides.severity_of(catalog(), "if-arity"),
            Severity::Error
        );
    }

    #[test]
    fn a_rule_selector_promotes_exactly_that_rule() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(catalog(), "redundant-quote", Severity::Error, &[])
            .expect("known rule");
        assert_eq!(
            overrides.severity_of(catalog(), "redundant-quote"),
            Severity::Error
        );
        assert_eq!(
            overrides.severity_of(catalog(), "nested-progn"),
            Severity::Warning
        );
    }

    #[test]
    fn a_category_selector_reaches_every_rule_in_it() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(catalog(), "suspicious", Severity::Error, &[])
            .expect("known category");
        assert_eq!(
            overrides.severity_of(catalog(), "redundant-quote"),
            Severity::Error
        );
        assert_eq!(
            overrides.severity_of(catalog(), "nested-progn"),
            Severity::Error
        );
        // Another category is untouched.
        assert_eq!(
            overrides.severity_of(catalog(), "if-arity"),
            Severity::Error
        );
    }

    #[test]
    fn a_later_rule_selector_wins_over_an_earlier_category_one() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(catalog(), "suspicious", Severity::Error, &[])
            .expect("known category");
        overrides
            .apply(catalog(), "redundant-quote", Severity::Warning, &[])
            .expect("known rule");
        assert_eq!(
            overrides.severity_of(catalog(), "redundant-quote"),
            Severity::Warning
        );
        assert_eq!(
            overrides.severity_of(catalog(), "nested-progn"),
            Severity::Error
        );
    }

    #[test]
    fn an_unknown_selector_is_rejected_with_the_valid_names() {
        let mut overrides = SeverityOverrides::new();
        let error = overrides
            .apply(catalog(), "redundant-quotes", Severity::Error, &[])
            .expect_err("typo must not pass silently");
        let RuleSelectionError::UnknownRule {
            name,
            valid,
            suggestion,
        } = error
        else {
            panic!("expected an unknown-rule error");
        };
        assert_eq!(name, "redundant-quotes");
        assert!(valid.contains("redundant-quote"));
        assert!(valid.contains("suspicious"));
        assert_eq!(suggestion, r#"; did you mean "redundant-quote"?"#);
    }

    #[test]
    fn entries_report_what_a_run_applied() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(catalog(), "suspicious", Severity::Error, &[])
            .expect("known category");
        let listed: Vec<(&str, Severity)> = overrides.entries().collect();
        assert_eq!(
            listed,
            vec![
                ("nested-progn", Severity::Error),
                ("redundant-quote", Severity::Error),
            ]
        );
    }

    #[test]
    fn a_selector_naming_a_loaded_custom_rule_promotes_it() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(
                catalog(),
                "my-custom-rule",
                Severity::Error,
                &["my-custom-rule"],
            )
            .expect("known custom rule");
        // `severity_of` — the catalogue-backed reading — has nothing to fall
        // back to for a name it does not register, but the override itself is
        // recorded and answerable directly.
        assert_eq!(
            overrides.override_of("my-custom-rule"),
            Some(Severity::Error)
        );
    }

    #[test]
    fn a_selector_matching_neither_the_catalogue_nor_a_custom_rule_is_still_unknown() {
        let mut overrides = SeverityOverrides::new();
        let error = overrides
            .apply(
                catalog(),
                "not-a-real-rule",
                Severity::Error,
                &["my-custom-rule"],
            )
            .expect_err("typo must not pass silently");
        let RuleSelectionError::UnknownRule { name, valid, .. } = error else {
            panic!("expected an unknown-rule error");
        };
        assert_eq!(name, "not-a-real-rule");
        assert!(valid.contains("my-custom-rule"));
    }

    #[test]
    fn override_of_answers_none_with_no_override_recorded() {
        let overrides = SeverityOverrides::new();
        assert_eq!(overrides.override_of("redundant-quote"), None);
    }

    #[test]
    fn seed_gives_severity_of_an_answer_with_no_catalogue_entry() {
        let mut overrides = SeverityOverrides::new();
        overrides.seed("my-custom-rule", Severity::Warning);
        assert_eq!(
            overrides.severity_of(catalog(), "my-custom-rule"),
            Severity::Warning
        );
    }

    #[test]
    fn an_explicit_deny_still_wins_over_a_seeded_default() {
        let mut overrides = SeverityOverrides::new();
        overrides.seed("my-custom-rule", Severity::Warning);
        overrides
            .apply(
                catalog(),
                "my-custom-rule",
                Severity::Error,
                &["my-custom-rule"],
            )
            .expect("known custom rule");
        assert_eq!(
            overrides.severity_of(catalog(), "my-custom-rule"),
            Severity::Error
        );
    }

    #[test]
    fn seeding_after_an_explicit_deny_does_not_overwrite_it() {
        let mut overrides = SeverityOverrides::new();
        overrides
            .apply(
                catalog(),
                "my-custom-rule",
                Severity::Error,
                &["my-custom-rule"],
            )
            .expect("known custom rule");
        overrides.seed("my-custom-rule", Severity::Warning);
        assert_eq!(
            overrides.severity_of(catalog(), "my-custom-rule"),
            Severity::Error
        );
    }
}
