//! The published rule catalogue, derived from [`super::REGISTRY`].
//!
//! Every constant here is computed at compile time by walking the registry.
//! That is the whole point: the four arrays these replace were maintained by
//! hand and could disagree — a rule present in `RULES` but missing from
//! `RULE_DOCS`, or listed in `FIXABLE_RULES` without the fix engine ever
//! producing one. There is now a single array, and the `const` assertions
//! below pin each derived length so that gaining or losing a rule is a
//! deliberate change.

use crate::domain::lint::model::{RuleCategory, Severity};

use super::{REGISTRY, RULE_COUNT};

/// Stable rule identifiers, matching each lint's own `inspect` command name.
pub const RULES: [&str; RULE_COUNT] = {
    let mut names = [""; RULE_COUNT];
    let mut index = 0;
    while index < RULE_COUNT {
        names[index] = REGISTRY[index].meta().name().as_str();
        index += 1;
    }
    names
};

/// The category names accepted by `--category`.
pub const CATEGORIES: [&str; RuleCategory::ALL.len()] = {
    let mut names = [""; RuleCategory::ALL.len()];
    let mut index = 0;
    while index < RuleCategory::ALL.len() {
        names[index] = RuleCategory::ALL[index].as_str();
        index += 1;
    }
    names
};

/// `(rule name, category, one-line description)` for each rule, in [`RULES`]
/// order. Powers `inspect lint --list-rules`, the `--category` filter, and
/// inline descriptions in the report, so an agent can discover the rule set,
/// its groupings, and its `--rule`/`--exclude`/`--category` names without
/// consulting the documentation.
pub const RULE_DOCS: [(&str, &str, &str); RULE_COUNT] = {
    let mut docs = [("", "", ""); RULE_COUNT];
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        docs[index] = (
            meta.name().as_str(),
            meta.category().as_str(),
            meta.description(),
        );
        index += 1;
    }
    docs
};

const fn fixable_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().fixability().is_fixable() {
            count += 1;
        }
        index += 1;
    }
    count
}

const fn warning_count() -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().severity().is_warning() {
            count += 1;
        }
        index += 1;
    }
    count
}

/// The rules for which `inspect lint --fix` (and the SARIF `fixes` field) can
/// synthesize an automatic rewrite. The rest are diagnostic-only: their repair
/// depends on intent a machine cannot infer.
pub const FIXABLE_RULES: [&str; fixable_count()] = {
    let mut names = [""; fixable_count()];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.fixability().is_fixable() {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

/// The rules whose findings are warnings (correct-but-redundant/style code).
/// Every other rule is an `error` — a likely or certain bug.
pub const WARNING_RULES: [&str; warning_count()] = {
    let mut names = [""; warning_count()];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.severity().is_warning() {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

// The suite's shape, pinned. A rule added or removed without updating these is
// a compile error rather than a silently different report.
const _: () = assert!(RULE_COUNT == 134);
const _: () = assert!(fixable_count() == 88);
const _: () = assert!(warning_count() == 93);

fn meta_of(name: &str) -> Option<&'static crate::domain::lint::model::RuleMeta> {
    REGISTRY
        .iter()
        .map(super::RuleEntry::meta)
        .find(|meta| meta.name().as_str() == name)
}

/// The one-line description for a rule name, or `None` if the name is unknown.
pub fn rule_description(name: &str) -> Option<&'static str> {
    meta_of(name).map(|meta| meta.description())
}

/// The category for a rule name, or `None` if the name is unknown.
pub fn rule_category(name: &str) -> Option<RuleCategory> {
    meta_of(name).map(|meta| meta.category())
}

/// Whether `inspect lint --fix` can repair this rule's findings.
pub fn rule_is_fixable(name: &str) -> bool {
    meta_of(name).is_some_and(|meta| meta.fixability().is_fixable())
}

/// The severity of a rule's findings (`error` unless it is a style rule).
///
/// An unknown name reports `Error`, matching the historical `contains`-based
/// lookup that treated anything not listed as a warning as an error.
pub fn rule_severity(name: &str) -> Severity {
    meta_of(name).map_or(Severity::Error, |meta| meta.severity())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_name_is_unique() {
        let mut names = RULES.to_vec();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn rule_docs_stay_in_lockstep_with_rules() {
        let names: Vec<&str> = RULE_DOCS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names, RULES.to_vec());
        for (name, category, description) in RULE_DOCS {
            assert!(
                CATEGORIES.contains(&category),
                "{name} has a stray category"
            );
            assert!(!description.is_empty(), "{name} has no description");
        }
    }

    #[test]
    fn derived_subsets_are_subsets_in_rules_order() {
        let fixable: Vec<&str> = RULES
            .iter()
            .copied()
            .filter(|rule| rule_is_fixable(rule))
            .collect();
        assert_eq!(fixable, FIXABLE_RULES.to_vec());
        let warnings: Vec<&str> = RULES
            .iter()
            .copied()
            .filter(|rule| rule_severity(rule) == Severity::Warning)
            .collect();
        assert_eq!(warnings, WARNING_RULES.to_vec());
    }

    #[test]
    fn an_unknown_rule_name_resolves_to_nothing() {
        assert_eq!(rule_description("no-such-rule"), None);
        assert_eq!(rule_category("no-such-rule"), None);
        assert!(!rule_is_fixable("no-such-rule"));
    }
}
