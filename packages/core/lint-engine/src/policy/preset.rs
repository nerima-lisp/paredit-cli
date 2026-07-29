//! The three-step ruleset ladder `--preset` selects.
//!
//! With 143 rules and counting, "all of them" is the wrong default for two
//! opposite reasons at once: a project adopting the tool wants the findings it
//! will certainly act on, and a project that has adopted it wants the ones it
//! has decided to care about. Neither is served by a flat list plus a growing
//! `--exclude` line in CI.
//!
//! A preset answers that from the metadata rules already carry, so it needs no
//! second list to maintain: [`super::super::model::Severity`] separates "likely
//! bug" from "style", and [`super::super::model::RuleTag`] separates stable
//! from experimental and neutral from opinionated. A rule joins the right
//! preset by describing itself accurately, not by being named somewhere.
//!
//! [`RulePreset::Recommended`] is the default and is deliberately the preset
//! that reproduces the historical "every rule" behaviour for every rule that
//! existed before presets did: the tags that narrow it are opt-in, so no
//! existing rule silently left the default set.

use crate::model::{RuleMeta, RuleTag, Severity};

/// How wide a net a run casts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RulePreset {
    /// Likely and certain bugs only: error severity, nothing opinionated.
    /// The set worth failing an unfamiliar codebase's build on.
    Minimal,
    /// Everything stable and uncontroversial. The default.
    #[default]
    Recommended,
    /// Adds the opinionated rules — naming conventions, mandatory docstrings,
    /// and the rest of the "correct, but only if you have agreed to it" set.
    Pedantic,
    /// Every registered rule, experimental ones included.
    All,
}

impl RulePreset {
    /// Every preset, in widening order — which is also the order
    /// `--list-presets` prints them.
    pub const ALL: [Self; 4] = [Self::Minimal, Self::Recommended, Self::Pedantic, Self::All];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Recommended => "recommended",
            Self::Pedantic => "pedantic",
            Self::All => "all",
        }
    }

    /// Not `FromStr`, for the same reason [`super::super::model::RuleTag::parse`]
    /// is not: the caller has a better message than a parse error.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.as_str() == value)
    }

    /// One line describing what the preset admits, for `--list-presets`.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Minimal => "error-severity rules only: likely and certain bugs",
            Self::Recommended => "every stable rule that is not opinionated (the default)",
            Self::Pedantic => "adds naming, documentation, and other convention rules",
            Self::All => "every registered rule, including experimental ones",
        }
    }

    /// Whether `meta` belongs to this preset.
    ///
    /// `experimental` is the caller's separate `--experimental` opt-in: it
    /// widens every preset by the experimental rules rather than being a fifth
    /// preset, because "the recommended set, plus what is being trialled" is a
    /// combination someone genuinely wants and a ladder cannot express.
    #[must_use]
    pub fn admits(self, meta: &RuleMeta, experimental: bool) -> bool {
        if meta.has_tag(RuleTag::Experimental) && !(experimental || self == Self::All) {
            return false;
        }
        match self {
            Self::All => true,
            Self::Pedantic => true,
            Self::Recommended => !meta.has_tag(RuleTag::Pedantic),
            Self::Minimal => !meta.has_tag(RuleTag::Pedantic) && meta.severity() == Severity::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fixability, RuleCategory};

    fn meta(severity: Severity, tags: &[RuleTag]) -> RuleMeta {
        RuleMeta::new(
            "sample",
            RuleCategory::Suspicious,
            severity,
            "a sample rule",
            Fixability::ReportOnly,
        )
        .with_tags(tags)
    }

    #[test]
    fn a_plain_error_rule_is_in_every_preset() {
        let rule = meta(Severity::Error, &[]);
        for preset in RulePreset::ALL {
            assert!(preset.admits(&rule, false), "{}", preset.as_str());
        }
    }

    #[test]
    fn minimal_drops_warnings_and_keeps_errors() {
        assert!(!RulePreset::Minimal.admits(&meta(Severity::Warning, &[]), false));
        assert!(RulePreset::Minimal.admits(&meta(Severity::Error, &[]), false));
    }

    #[test]
    fn recommended_keeps_warnings_and_drops_the_opinionated() {
        assert!(RulePreset::Recommended.admits(&meta(Severity::Warning, &[]), false));
        assert!(
            !RulePreset::Recommended.admits(&meta(Severity::Warning, &[RuleTag::Pedantic]), false)
        );
    }

    #[test]
    fn pedantic_admits_the_opinionated_but_still_not_the_experimental() {
        let opinionated = meta(Severity::Warning, &[RuleTag::Pedantic]);
        assert!(RulePreset::Pedantic.admits(&opinionated, false));
        let trial = meta(Severity::Error, &[RuleTag::Experimental]);
        assert!(!RulePreset::Pedantic.admits(&trial, false));
    }

    #[test]
    fn the_experimental_opt_in_widens_any_preset() {
        let trial = meta(Severity::Error, &[RuleTag::Experimental]);
        assert!(RulePreset::Minimal.admits(&trial, true));
        assert!(RulePreset::Recommended.admits(&trial, true));
    }

    #[test]
    fn the_experimental_opt_in_does_not_override_the_other_filters() {
        // Opting into experiments must not smuggle in an opinionated rule that
        // `minimal` would otherwise refuse.
        let both = meta(
            Severity::Warning,
            &[RuleTag::Experimental, RuleTag::Pedantic],
        );
        assert!(!RulePreset::Minimal.admits(&both, true));
        assert!(!RulePreset::Recommended.admits(&both, true));
        assert!(RulePreset::Pedantic.admits(&both, true));
    }

    #[test]
    fn the_all_preset_needs_no_opt_in() {
        let trial = meta(
            Severity::Warning,
            &[RuleTag::Experimental, RuleTag::Pedantic],
        );
        assert!(RulePreset::All.admits(&trial, false));
    }

    #[test]
    fn wire_names_round_trip_and_the_default_is_recommended() {
        for preset in RulePreset::ALL {
            assert_eq!(RulePreset::parse(preset.as_str()), Some(preset));
        }
        assert_eq!(RulePreset::parse("nonesuch"), None);
        assert_eq!(RulePreset::default(), RulePreset::Recommended);
    }
}
