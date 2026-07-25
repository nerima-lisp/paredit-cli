//! Choosing which rules a run applies.

use anyhow::Result;

use crate::domain::lint::model::{RuleCategory, RuleName};
use crate::domain::lint::registry::catalog::{CATEGORIES, RULES, rule_category};

/// Which rules a dispatch pass should run.
///
/// `inspect lint` needs both readings: a report computes *every* rule and
/// filters afterwards, while `--fix` must only ever synthesize rewrites for the
/// rules the caller selected. Making that a parameter rather than a
/// post-filter is what keeps an excluded rule from silently editing a file.
#[derive(Debug, Clone, Copy)]
pub enum RuleSelection<'a> {
    /// Every registered rule.
    All,
    /// Only the named rules, as resolved by [`resolve_active_rules`].
    Only(&'a [&'a str]),
}

impl RuleSelection<'_> {
    pub fn includes(self, rule: RuleName) -> bool {
        match self {
            Self::All => true,
            Self::Only(names) => names.contains(&rule.as_str()),
        }
    }
}

/// Resolves the active rule set for one run.
///
/// The included set is the rules named by `only`, or every rule in the
/// categories named by `categories`, or (when both are empty) all of [`RULES`];
/// `exclude` then removes rules from it. `only` and `categories` are mutually
/// exclusive at the CLI layer. Every named rule must be one of [`RULES`] and
/// every named category one of [`CATEGORIES`] — an unknown name is a hard error
/// rather than a silent no-op, so a typo in CI fails loudly. The result
/// preserves [`RULES`] order.
pub fn resolve_active_rules(
    only: &[String],
    exclude: &[String],
    categories: &[String],
) -> Result<Vec<&'static str>> {
    for name in only.iter().chain(exclude) {
        if !RULES.contains(&name.as_str()) {
            anyhow::bail!(
                "unknown lint rule {name:?}; valid rules: {}",
                RULES.join(", ")
            );
        }
    }
    for name in categories {
        if !CATEGORIES.contains(&name.as_str()) {
            anyhow::bail!(
                "unknown lint category {name:?}; valid categories: {}",
                CATEGORIES.join(", ")
            );
        }
    }

    Ok(RULES
        .iter()
        .copied()
        .filter(|rule| {
            let included = if !only.is_empty() {
                only.iter().any(|name| name == rule)
            } else if !categories.is_empty() {
                rule_category(rule).is_some_and(|category: RuleCategory| {
                    categories.iter().any(|name| name == category.as_str())
                })
            } else {
                true
            };
            included && !exclude.iter().any(|name| name == rule)
        })
        .collect())
}
