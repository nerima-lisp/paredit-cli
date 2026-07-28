//! The compile-time description of one lint rule.

use super::{
    Fixability, RuleCategory, RuleExplanation, RuleName, RuleSetting, RuleTag, RuleTags, Severity,
};

/// Everything about a rule that is known without looking at any source: its
/// identity, grouping, seriousness, documentation, tunable knobs, and whether
/// it can repair what it finds.
///
/// This is the single source of truth the public `RULES`, `RULE_DOCS`,
/// `FIXABLE_RULES`, and `WARNING_RULES` constants are derived from. Every
/// accessor is `const` so that derivation happens at compile time: the four
/// arrays cannot drift out of lockstep because there is only one array.
///
/// The optional facets — tags, long-form explanation, settings — are added by
/// `const` builder methods rather than by widening [`RuleMeta::new`]. A sixth
/// positional parameter would have to be threaded through every rule in the
/// suite to add a property most rules do not have, and the next optional facet
/// would need a seventh. Chaining keeps a rule's `META` stating only what is
/// true of it.
#[derive(Debug)]
pub struct RuleMeta {
    name: RuleName,
    category: RuleCategory,
    severity: Severity,
    description: &'static str,
    fixability: Fixability,
    tags: RuleTags,
    explanation: Option<RuleExplanation>,
    settings: &'static [RuleSetting],
}

impl RuleMeta {
    #[must_use]
    pub const fn new(
        name: &'static str,
        category: RuleCategory,
        severity: Severity,
        description: &'static str,
        fixability: Fixability,
    ) -> Self {
        assert!(
            !description.is_empty(),
            "a lint rule needs a one-line description for --list-rules"
        );
        Self {
            name: RuleName::new(name),
            category,
            severity,
            description,
            fixability,
            tags: RuleTags::NONE,
            explanation: None,
            settings: &[],
        }
    }

    /// Declares the rule's orthogonal properties, which drive `--tag`, the
    /// presets, and `--experimental`.
    #[must_use]
    pub const fn with_tags(mut self, tags: &[RuleTag]) -> Self {
        self.tags = RuleTags::of(tags);
        self
    }

    /// Attaches the long-form documentation `--explain` prints.
    #[must_use]
    pub const fn with_explanation(mut self, explanation: RuleExplanation) -> Self {
        self.explanation = Some(explanation);
        self
    }

    /// Declares the rule's tunable thresholds, which is what makes
    /// `--rule-arg <rule>.<key>=<value>` checkable before any file is read.
    #[must_use]
    pub const fn with_settings(mut self, settings: &'static [RuleSetting]) -> Self {
        assert!(
            !settings.is_empty(),
            "declare settings or omit the call; an empty declaration says nothing"
        );
        self.settings = settings;
        self
    }

    #[must_use]
    pub const fn name(&self) -> RuleName {
        self.name
    }

    #[must_use]
    pub const fn category(&self) -> RuleCategory {
        self.category
    }

    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }

    #[must_use]
    pub const fn fixability(&self) -> Fixability {
        self.fixability
    }

    #[must_use]
    pub const fn tags(&self) -> RuleTags {
        self.tags
    }

    #[must_use]
    pub const fn has_tag(&self, tag: RuleTag) -> bool {
        self.tags.contains(tag)
    }

    /// The long-form documentation, or `None` for a rule whose one-line
    /// description is all there is. `--explain` still works for those: it falls
    /// back to the metadata every rule has.
    #[must_use]
    pub const fn explanation(&self) -> Option<RuleExplanation> {
        self.explanation
    }

    /// The rule's tunable knobs, empty for the rules that have none.
    #[must_use]
    pub const fn settings(&self) -> &'static [RuleSetting] {
        self.settings
    }

    /// The knob named `key`, or `None` — which is exactly the check
    /// `--rule-arg` performs before a run starts.
    #[must_use]
    pub fn setting(&self, key: &str) -> Option<RuleSetting> {
        self.settings
            .iter()
            .copied()
            .find(|setting| setting.key() == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: RuleMeta = RuleMeta::new(
        "zero-divisor",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a division-family form with a literal 0 divisor",
        Fixability::ReportOnly,
    );

    #[test]
    fn exposes_every_field_in_a_const_context() {
        const NAME: &str = SAMPLE.name().as_str();
        const CATEGORY: &str = SAMPLE.category().as_str();
        assert_eq!(NAME, "zero-divisor");
        assert_eq!(CATEGORY, "suspicious");
        assert_eq!(SAMPLE.fixability(), Fixability::ReportOnly);
        assert_eq!(SAMPLE.severity(), Severity::Warning);
    }

    #[test]
    fn a_rule_that_declares_no_optional_facet_carries_none() {
        assert!(SAMPLE.tags().is_empty());
        assert_eq!(SAMPLE.explanation(), None);
        assert!(SAMPLE.settings().is_empty());
        assert_eq!(SAMPLE.setting("max"), None);
    }

    const DECORATED: RuleMeta = RuleMeta::new(
        "deep-nesting",
        RuleCategory::Suspicious,
        Severity::Warning,
        "a form nested past the configured depth",
        Fixability::ReportOnly,
    )
    .with_tags(&[RuleTag::Pedantic, RuleTag::Style])
    .with_explanation(RuleExplanation::new("Deep nesting hides the control flow."))
    .with_settings(&[RuleSetting::new("max", 5, "the deepest acceptable nesting")]);

    #[test]
    fn the_builders_compose_in_a_const_context() {
        const { assert!(DECORATED.has_tag(RuleTag::Pedantic)) };
        assert!(DECORATED.has_tag(RuleTag::Style));
        assert!(!DECORATED.has_tag(RuleTag::Experimental));
        assert_eq!(
            DECORATED.explanation().expect("explanation").rationale(),
            "Deep nesting hides the control flow."
        );
        assert_eq!(DECORATED.setting("max").expect("knob").default(), 5);
        assert_eq!(DECORATED.setting("min"), None);
    }

    #[test]
    fn the_builders_leave_the_required_fields_alone() {
        assert_eq!(DECORATED.name().as_str(), "deep-nesting");
        assert_eq!(DECORATED.severity(), Severity::Warning);
        assert_eq!(DECORATED.fixability(), Fixability::ReportOnly);
    }
}
