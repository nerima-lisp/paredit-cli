//! Choosing which rules a run applies.

use crate::error::RuleSelectionError;
use crate::model::{RuleCategory, RuleName};
use crate::rule::RuleCatalog;

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
    #[must_use]
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
/// categories named by `categories`, or (when both are empty) all of the catalogue;
/// `exclude` then removes rules from it. `only` and `categories` are mutually
/// exclusive at the CLI layer. Every named rule must be one of the catalogue's rules and
/// every named category one of the catalogue's categories — an unknown name is a hard error
/// rather than a silent no-op, so a typo in CI fails loudly. The result
/// preserves the catalogue's registration order.
pub fn resolve_active_rules(
    catalog: RuleCatalog,
    only: &[String],
    exclude: &[String],
    categories: &[String],
) -> Result<Vec<&'static str>, RuleSelectionError> {
    let rules: Vec<&'static str> = catalog.names().collect();
    let category_names = catalog.categories();
    for name in only.iter().chain(exclude) {
        if !rules.contains(&name.as_str()) {
            return Err(RuleSelectionError::UnknownRule {
                name: name.clone(),
                valid: rules.join(", "),
            });
        }
    }
    for name in categories {
        if !category_names.contains(&name.as_str()) {
            return Err(RuleSelectionError::UnknownCategory {
                name: name.clone(),
                valid: category_names.join(", "),
            });
        }
    }

    Ok(rules
        .into_iter()
        .filter(|rule| {
            let included = if !only.is_empty() {
                only.iter().any(|name| name == rule)
            } else if !categories.is_empty() {
                catalog
                    .category_of(rule)
                    .is_some_and(|category: RuleCategory| {
                        categories.iter().any(|name| name == category.as_str())
                    })
            } else {
                true
            };
            included && !exclude.iter().any(|name| name == rule)
        })
        .collect())
}
