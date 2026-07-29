//! Choosing which rules a run applies.

use crate::error::RuleSelectionError;
use crate::model::{RuleCategory, RuleName, RuleTag};
use crate::rule::RuleCatalog;

use super::preset::RulePreset;

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

/// Everything the caller said about which rules to run.
///
/// A struct rather than six positional parameters, because the selectors do not
/// compose freely — `only` overrides `categories`, the preset floors both — and
/// a caller reading `resolve_active_rules(c, &[], &x, &[], &[], p, false)`
/// cannot see which argument is which. Naming them also lets a new selector be
/// added without touching every call site.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuleFilter<'a> {
    /// `--rule`: run only these. Mutually exclusive with `categories` at the
    /// CLI layer.
    pub only: &'a [String],
    /// `--exclude` and `--allow`: never run these.
    pub exclude: &'a [String],
    /// `--category`: run only rules in these families.
    pub categories: &'a [String],
    /// `--tag`: run only rules carrying *all* of these tags.
    pub tags: &'a [String],
    /// `--preset`: the ladder rung, applied on top of everything above.
    pub preset: RulePreset,
    /// `--experimental`: widen the preset by the experimental rules.
    pub experimental: bool,
}

impl<'a> RuleFilter<'a> {
    /// The historical three selectors, for callers with no preset opinion.
    #[must_use]
    pub const fn named(
        only: &'a [String],
        exclude: &'a [String],
        categories: &'a [String],
    ) -> Self {
        Self {
            only,
            exclude,
            categories,
            tags: &[],
            preset: RulePreset::Recommended,
            experimental: false,
        }
    }
}

/// Resolves the active rule set for one run.
///
/// The set is narrowed in four independent steps, each of which can only
/// remove:
///
/// 1. the *inclusion* selector — `only`, else `categories`, else everything;
/// 2. `tags`, which requires every named tag to be present;
/// 3. the preset (plus the `experimental` opt-in);
/// 4. `exclude`, which has the last word.
///
/// Every named rule, category, and tag must be one the catalogue knows — an
/// unknown name is a hard error rather than a silent no-op, so a typo in CI
/// fails loudly. The result preserves the catalogue's registration order.
///
/// `only` is deliberately *not* exempt from the preset: `--rule` names a rule
/// but does not assert it is stable, and a run that silently included an
/// experimental rule because it was named would make `--preset` untrustworthy.
/// Naming an experimental rule without `--experimental` therefore resolves to
/// nothing, which is visible, rather than to a rule the preset excluded.
pub fn resolve_active_rules(
    catalog: RuleCatalog,
    filter: &RuleFilter<'_>,
) -> Result<Vec<&'static str>, RuleSelectionError> {
    let rules: Vec<&'static str> = catalog.names().collect();
    let category_names = catalog.categories();
    for name in filter.only.iter().chain(filter.exclude) {
        if !rules.contains(&name.as_str()) {
            return Err(RuleSelectionError::UnknownRule {
                name: name.clone(),
                valid: rules.join(", "),
            });
        }
    }
    for name in filter.categories {
        if !category_names.contains(&name.as_str()) {
            return Err(RuleSelectionError::UnknownCategory {
                name: name.clone(),
                valid: category_names.join(", "),
            });
        }
    }
    let mut wanted_tags = Vec::with_capacity(filter.tags.len());
    for name in filter.tags {
        let tag = RuleTag::parse(name).ok_or_else(|| RuleSelectionError::UnknownTag {
            name: name.clone(),
            valid: RuleTag::ALL
                .iter()
                .map(|tag| tag.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        })?;
        wanted_tags.push(tag);
    }

    Ok(catalog
        .entries()
        .iter()
        .map(crate::rule::RuleEntry::meta)
        .filter(|meta| {
            let rule = meta.name().as_str();
            let included = if !filter.only.is_empty() {
                filter.only.iter().any(|name| name == rule)
            } else if !filter.categories.is_empty() {
                let category: RuleCategory = meta.category();
                filter
                    .categories
                    .iter()
                    .any(|name| name == category.as_str())
            } else {
                true
            };
            included
                && wanted_tags.iter().all(|tag| meta.has_tag(*tag))
                && filter.preset.admits(meta, filter.experimental)
                && !filter.exclude.iter().any(|name| name == rule)
        })
        .map(|meta| meta.name().as_str())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fixability, RuleMeta, Severity};
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

    static STABLE: RuleMeta = RuleMeta::new(
        "stable-rule",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a stable rule",
        Fixability::ReportOnly,
    );
    static BUG: RuleMeta = RuleMeta::new(
        "bug-rule",
        RuleCategory::Arity,
        Severity::Error,
        "a certain bug",
        Fixability::ReportOnly,
    );
    static OPINIONATED: RuleMeta = RuleMeta::new(
        "naming-rule",
        RuleCategory::Naming,
        Severity::Warning,
        "an opinionated rule",
        Fixability::ReportOnly,
    )
    .with_tags(&[RuleTag::Pedantic, RuleTag::Style]);
    static TRIAL: RuleMeta = RuleMeta::new(
        "trial-rule",
        RuleCategory::Security,
        Severity::Error,
        "an experimental rule",
        Fixability::ReportOnly,
    )
    .with_tags(&[RuleTag::Experimental]);

    static ENTRIES: [RuleEntry; 4] = [
        RuleEntry::new(&STABLE, &NOOP),
        RuleEntry::new(&BUG, &NOOP),
        RuleEntry::new(&OPINIONATED, &NOOP),
        RuleEntry::new(&TRIAL, &NOOP),
    ];

    fn catalog() -> RuleCatalog {
        RuleCatalog::new(&ENTRIES)
    }

    fn resolve(filter: &RuleFilter<'_>) -> Vec<&'static str> {
        resolve_active_rules(catalog(), filter).expect("resolve")
    }

    #[test]
    fn the_default_filter_is_the_recommended_preset() {
        assert_eq!(
            resolve(&RuleFilter::default()),
            vec!["stable-rule", "bug-rule"]
        );
    }

    #[test]
    fn the_all_preset_includes_every_rule_in_registration_order() {
        let filter = RuleFilter {
            preset: RulePreset::All,
            ..RuleFilter::default()
        };
        assert_eq!(
            resolve(&filter),
            vec!["stable-rule", "bug-rule", "naming-rule", "trial-rule"]
        );
    }

    #[test]
    fn minimal_keeps_only_the_error_rules() {
        let filter = RuleFilter {
            preset: RulePreset::Minimal,
            ..RuleFilter::default()
        };
        assert_eq!(resolve(&filter), vec!["bug-rule"]);
    }

    #[test]
    fn a_tag_filter_requires_every_named_tag() {
        let both = ["pedantic".to_owned(), "style".to_owned()];
        let filter = RuleFilter {
            tags: &both,
            preset: RulePreset::Pedantic,
            ..RuleFilter::default()
        };
        assert_eq!(resolve(&filter), vec!["naming-rule"]);

        let unmatched = ["pedantic".to_owned(), "destructive".to_owned()];
        let filter = RuleFilter {
            tags: &unmatched,
            preset: RulePreset::Pedantic,
            ..RuleFilter::default()
        };
        assert!(resolve(&filter).is_empty());
    }

    #[test]
    fn exclude_has_the_last_word_over_only() {
        let only = ["stable-rule".to_owned(), "bug-rule".to_owned()];
        let exclude = ["bug-rule".to_owned()];
        let filter = RuleFilter {
            only: &only,
            exclude: &exclude,
            ..RuleFilter::default()
        };
        assert_eq!(resolve(&filter), vec!["stable-rule"]);
    }

    #[test]
    fn naming_an_experimental_rule_is_not_an_opt_in() {
        let only = ["trial-rule".to_owned()];
        let filter = RuleFilter {
            only: &only,
            ..RuleFilter::default()
        };
        assert!(resolve(&filter).is_empty());

        let filter = RuleFilter {
            only: &only,
            experimental: true,
            ..RuleFilter::default()
        };
        assert_eq!(resolve(&filter), vec!["trial-rule"]);
    }

    #[test]
    fn a_category_selector_still_obeys_the_preset() {
        let categories = ["naming".to_owned()];
        let filter = RuleFilter {
            categories: &categories,
            ..RuleFilter::default()
        };
        assert!(resolve(&filter).is_empty());

        let filter = RuleFilter {
            categories: &categories,
            preset: RulePreset::Pedantic,
            ..RuleFilter::default()
        };
        assert_eq!(resolve(&filter), vec!["naming-rule"]);
    }

    #[test]
    fn an_unknown_tag_is_rejected_with_the_valid_names() {
        let tags = ["experimentl".to_owned()];
        let filter = RuleFilter {
            tags: &tags,
            ..RuleFilter::default()
        };
        let error = resolve_active_rules(catalog(), &filter).expect_err("typo must not pass");
        let RuleSelectionError::UnknownTag { name, valid } = error else {
            panic!("expected an unknown-tag error");
        };
        assert_eq!(name, "experimentl");
        assert!(valid.contains("experimental"));
    }

    #[test]
    fn unknown_rules_and_categories_are_still_rejected() {
        let only = ["nope".to_owned()];
        assert!(
            resolve_active_rules(
                catalog(),
                &RuleFilter {
                    only: &only,
                    ..RuleFilter::default()
                }
            )
            .is_err()
        );
        let categories = ["nope".to_owned()];
        assert!(
            resolve_active_rules(
                catalog(),
                &RuleFilter {
                    categories: &categories,
                    ..RuleFilter::default()
                }
            )
            .is_err()
        );
    }
}
