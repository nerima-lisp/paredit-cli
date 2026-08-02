//! The published rule catalogue, derived from [`super::REGISTRY`].
//!
//! Every constant here is computed at compile time by walking the registry.
//! That is the whole point: the four arrays these replace were maintained by
//! hand and could disagree — a rule present in `RULES` but missing from
//! `RULE_DOCS`, or listed in `FIXABLE_RULES` without the fix engine ever
//! producing one. There is now a single array, and the `const` assertions
//! below pin each derived length so that gaining or losing a rule is a
//! deliberate change.

use crate::lint::model::{RuleCategory, RuleExplanation, RuleSetting, RuleTag, RuleTags, Severity};

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

/// The tag names accepted by `--tag`.
pub const TAGS: [&str; RuleTag::ALL.len()] = {
    let mut names = [""; RuleTag::ALL.len()];
    let mut index = 0;
    while index < RuleTag::ALL.len() {
        names[index] = RuleTag::ALL[index].as_str();
        index += 1;
    }
    names
};

const fn tagged_count(tag: RuleTag) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        if REGISTRY[index].meta().has_tag(tag) {
            count += 1;
        }
        index += 1;
    }
    count
}

/// The rules `--preset` keeps out of every rung but `all`, unless
/// `--experimental` is passed. Published so a caller can see what it is opting
/// into before opting in.
pub const EXPERIMENTAL_RULES: [&str; tagged_count(RuleTag::Experimental)] = {
    let mut names = [""; tagged_count(RuleTag::Experimental)];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.has_tag(RuleTag::Experimental) {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

/// The rules only `--preset pedantic` (and `all`) admits: correct, but
/// opinionated enough to be noise on a codebase that has not adopted the
/// convention.
pub const PEDANTIC_RULES: [&str; tagged_count(RuleTag::Pedantic)] = {
    let mut names = [""; tagged_count(RuleTag::Pedantic)];
    let mut written = 0;
    let mut index = 0;
    while index < RULE_COUNT {
        let meta = REGISTRY[index].meta();
        if meta.has_tag(RuleTag::Pedantic) {
            names[written] = meta.name().as_str();
            written += 1;
        }
        index += 1;
    }
    names
};

// The suite's shape, pinned. A rule added or removed without updating these is
// a compile error rather than a silently different report.
// 229 (through PR #82) + this branch's 37, spread over nine packages: 7
// (`lint-control-flow`), 6 (`lint-safety`), 5 (`lint-call-shape`), 4 each
// (`lint-conditional`, `lint-documentation`), 2 (`lint-contract-annotation`)
// and 3 each (`lint-performance`, `lint-portability`, `lint-introspection`) =
// 266.
//
// 37, not the 39 this branch first proposed. `check-type-redundant-with-declare`
// and `clojure-pre-referencing-percent` were dropped before merge once their
// premises were checked against the primary sources and refuted; both were
// false-positive generators on correct code. See
// `packages/feature/lint-contract-annotation/README.md`.
const _: () = assert!(RULE_COUNT == 266);
// Unchanged at 99: every one of this branch's 37 rules is
// `Fixability::ReportOnly`. Each one reports a judgment the tool cannot make
// on the author's behalf — whether an annotation or the parameter list under it
// is the wrong half, which of two nested `cond`s the author meant to keep, or
// how a temp file should be named are all decisions the author has to make,
// not spellings of one they already made. The two dropped rules were
// `ReportOnly` too, which is why this number does not move with them.
const _: () = assert!(fixable_count() == 99);
// 164 (through PR #82) + 31 of this branch's 37 rules. The other 6 are
// `Severity::Error`: `when-unless-implicit-nil-misused` and the five
// `lint-safety` rules that report an exploitable defect rather than a risk —
// `format-tilde-slash-unvalidated-function-designator`,
// `path-traversal-via-concatenated-filename`, `read-eval-star-rebound-to-t`,
// `sql-query-string-built-via-format` and
// `world-writable-file-mode-in-open-call`. Both dropped rules were `Warning`,
// so this fell by 2 where `fixable_count` did not.
const _: () = assert!(warning_count() == 195);
const _: () = assert!(EXPERIMENTAL_RULES.is_empty());
// 6 (through PR #82) + 8 of this branch's rules: `lint-call-shape`'s four
// threshold rules, whose limits are conventions a codebase either adopted or
// did not; `lint-documentation`'s `docstring-summary-line-too-long`,
// `missing-package-docstring` and `todo-fixme-no-attribution`; and
// `repeated-hash-table-lookup-same-key`, which is a real cost only on a hot
// path the rule cannot identify. Neither dropped rule was tagged `pedantic`,
// so this does not move either.
const _: () = assert!(PEDANTIC_RULES.len() == 14);

fn meta_of(name: &str) -> Option<&'static crate::lint::model::RuleMeta> {
    REGISTRY
        .iter()
        .map(super::RuleEntry::meta)
        .find(|meta| meta.name().as_str() == name)
}

/// The one-line description for a rule name, or `None` if the name is unknown.
#[must_use]
pub fn rule_description(name: &str) -> Option<&'static str> {
    meta_of(name).map(|meta| meta.description())
}

/// The category for a rule name, or `None` if the name is unknown.
#[must_use]
pub fn rule_category(name: &str) -> Option<RuleCategory> {
    meta_of(name).map(|meta| meta.category())
}

/// Whether `inspect lint --fix` can repair this rule's findings.
#[must_use]
pub fn rule_is_fixable(name: &str) -> bool {
    meta_of(name).is_some_and(|meta| meta.fixability().is_fixable())
}

/// The severity of a rule's findings (`error` unless it is a style rule).
///
/// An unknown name reports `Error`, matching the historical `contains`-based
/// lookup that treated anything not listed as a warning as an error.
#[must_use]
pub fn rule_severity(name: &str) -> Severity {
    meta_of(name).map_or(Severity::Error, |meta| meta.severity())
}

/// The orthogonal properties of a rule; empty for an unknown name and for the
/// majority of rules, which carry none.
#[must_use]
pub fn rule_tags(name: &str) -> RuleTags {
    meta_of(name).map_or(RuleTags::NONE, |meta| meta.tags())
}

/// The long-form documentation `--explain` prints, or `None` when the rule
/// supplies only its one-line description.
#[must_use]
pub fn rule_explanation(name: &str) -> Option<RuleExplanation> {
    meta_of(name).and_then(|meta| meta.explanation())
}

/// The tunable knobs a rule declares, empty for the rules that have none and
/// for an unknown name.
#[must_use]
pub fn rule_settings(name: &str) -> &'static [RuleSetting] {
    meta_of(name).map_or(&[], |meta| meta.settings())
}

/// The knob `key` of `rule`, or `None` — the lookup `--rule-arg` validates
/// against before a run starts.
#[must_use]
pub fn rule_setting(rule: &str, key: &str) -> Option<RuleSetting> {
    meta_of(rule).and_then(|meta| meta.setting(key))
}

/// The dialects a rule reports on, as wire names. Part of `--explain` because
/// "why did this rule find nothing?" is most often answered by the file's
/// dialect, not by the rule's logic.
#[must_use]
pub fn rule_dialects(name: &str) -> Vec<&'static str> {
    use paredit_core_syntax::dialect::Dialect;
    REGISTRY
        .iter()
        .find(|entry| entry.meta().name().as_str() == name)
        .map(|entry| {
            let scope = entry.rule().dialect_scope();
            Dialect::ALL
                .iter()
                .filter(|dialect| scope.includes(**dialect))
                .map(|dialect| dialect.label())
                .collect()
        })
        .unwrap_or_default()
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
