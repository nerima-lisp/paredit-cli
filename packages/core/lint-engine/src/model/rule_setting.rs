//! Per-rule thresholds a caller can retune without recompiling.
//!
//! A rule like "this form nests too deeply" has no defensible universal
//! constant in it. Hard-coding one makes the rule unusable for every project
//! that disagrees, and the usual escape — turn the rule off — throws away the
//! part everyone agrees on.
//!
//! So a rule *declares* its knobs (name, default, meaning) and reads them back
//! through [`RuleSettings`]. Declaring rather than parsing is what makes
//! `--rule-arg nesting-depth.max=6` checkable: an unknown rule or an unknown
//! key is rejected at argument-parsing time, against the declaration, before
//! any file is read. A rule that silently ignored an unrecognized key would
//! turn a typo in CI into a gate that quietly stopped gating.
//!
//! Values are `i64` because every knob the suite has is a count or a
//! threshold. When one is not, this grows a variant rather than a `String` that
//! every rule reparses.

use std::collections::BTreeMap;

/// One tunable knob a rule declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleSetting {
    key: &'static str,
    default: i64,
    description: &'static str,
}

impl RuleSetting {
    #[must_use]
    pub const fn new(key: &'static str, default: i64, description: &'static str) -> Self {
        assert!(!key.is_empty(), "a rule setting needs a key");
        assert!(
            !description.is_empty(),
            "a rule setting needs a description for --explain"
        );
        Self {
            key,
            default,
            description,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        self.key
    }

    #[must_use]
    pub const fn default(self) -> i64 {
        self.default
    }

    #[must_use]
    pub const fn description(self) -> &'static str {
        self.description
    }
}

/// The resolved settings for one run: every `--rule-arg` override, keyed by
/// `(rule, key)`.
///
/// Holds only the overrides. A rule asks for a key together with the default it
/// declared, so an absent entry needs no lookup into the registry and the map
/// stays empty — which it is for essentially every run.
#[derive(Debug, Clone, Default)]
pub struct RuleSettings {
    overrides: BTreeMap<(String, String), i64>,
}

impl RuleSettings {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The empty set, in a `const` context.
    ///
    /// [`crate::engine::RuleContext`] holds a `&RuleSettings` and is built once
    /// per file — including on the paths that have no overrides at all. A
    /// `const` empty lets those borrow a `static` instead of each constructing
    /// and dropping a map, and keeps `RuleContext::new` a `const fn`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            overrides: BTreeMap::new(),
        }
    }

    /// Records one override. A repeated `(rule, key)` keeps the last value,
    /// matching how a repeated flag reads on a command line.
    pub fn set(&mut self, rule: &str, key: &str, value: i64) {
        self.overrides
            .insert((rule.to_owned(), key.to_owned()), value);
    }

    /// The value for `setting` under `rule`, or the setting's declared default.
    #[must_use]
    pub fn get(&self, rule: &str, setting: RuleSetting) -> i64 {
        // The overwhelmingly common case, answered before anything allocates:
        // no `--rule-arg` was given at all, so there is nothing to find and the
        // declared default is the answer.
        //
        // The lookup below needs an owned `(String, String)` because that is
        // what the map is keyed by, so *every* read of a knob allocated twice
        // — including reads of an empty map. A rule that reads a knob once per
        // definition therefore paid two allocations per definition to learn a
        // constant, which on a file of a thousand definitions is two thousand
        // allocations for nothing. Three rules on the `clean/forms/*`
        // benchmarks do exactly that.
        //
        // Identical by construction: an empty map cannot contain the key, so
        // the branch below would have returned `setting.default()` too.
        if self.overrides.is_empty() {
            return setting.default();
        }
        self.overrides
            .get(&(rule.to_owned(), setting.key().to_owned()))
            .copied()
            .unwrap_or_else(|| setting.default())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }

    /// Every override as `(rule, key, value)`, sorted, for reporting what a run
    /// actually used.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str, i64)> {
        self.overrides
            .iter()
            .map(|((rule, key), value)| (rule.as_str(), key.as_str(), *value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_DEPTH: RuleSetting =
        RuleSetting::new("max", 5, "the deepest nesting reported as acceptable");

    #[test]
    fn an_unset_key_reads_its_declared_default() {
        let settings = RuleSettings::new();
        assert!(settings.is_empty());
        assert_eq!(settings.get("nesting-depth", MAX_DEPTH), 5);
    }

    #[test]
    fn an_override_wins_for_exactly_its_rule() {
        let mut settings = RuleSettings::new();
        settings.set("nesting-depth", "max", 9);
        assert_eq!(settings.get("nesting-depth", MAX_DEPTH), 9);
        // Another rule declaring a key spelled the same is untouched.
        assert_eq!(settings.get("function-length", MAX_DEPTH), 5);
    }

    #[test]
    fn the_last_override_of_a_key_wins() {
        let mut settings = RuleSettings::new();
        settings.set("nesting-depth", "max", 9);
        settings.set("nesting-depth", "max", 3);
        assert_eq!(settings.get("nesting-depth", MAX_DEPTH), 3);
    }

    #[test]
    fn entries_come_back_sorted_for_a_stable_report() {
        let mut settings = RuleSettings::new();
        settings.set("zeta", "max", 1);
        settings.set("alpha", "max", 2);
        let listed: Vec<(&str, &str, i64)> = settings.entries().collect();
        assert_eq!(listed, vec![("alpha", "max", 2), ("zeta", "max", 1)]);
    }
}
