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

    /// Applies one `--deny`/`--warn` selector, which names either a rule or a
    /// category.
    ///
    /// A name that is neither is a hard error rather than a silent no-op: the
    /// whole point of the flag is to change what fails CI, and a typo that
    /// quietly changed nothing would leave a build passing for the wrong
    /// reason.
    pub fn apply(
        &mut self,
        catalog: RuleCatalog,
        selector: &str,
        severity: Severity,
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
        let known: Vec<&str> = catalog.names().chain(catalog.categories()).collect();
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
            .apply(catalog(), "redundant-quote", Severity::Error)
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
            .apply(catalog(), "suspicious", Severity::Error)
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
            .apply(catalog(), "suspicious", Severity::Error)
            .expect("known category");
        overrides
            .apply(catalog(), "redundant-quote", Severity::Warning)
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
            .apply(catalog(), "redundant-quotes", Severity::Error)
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
            .apply(catalog(), "suspicious", Severity::Error)
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
}
