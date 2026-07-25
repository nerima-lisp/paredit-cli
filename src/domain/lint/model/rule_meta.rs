//! The compile-time description of one lint rule.

use super::{Fixability, RuleCategory, RuleName, Severity};

/// Everything about a rule that is known without looking at any source: its
/// identity, grouping, seriousness, one-line documentation, and whether it can
/// repair what it finds.
///
/// This is the single source of truth the public `RULES`, `RULE_DOCS`,
/// `FIXABLE_RULES`, and `WARNING_RULES` constants are derived from. Every
/// accessor is `const` so that derivation happens at compile time: the four
/// arrays cannot drift out of lockstep because there is only one array.
#[derive(Debug)]
pub struct RuleMeta {
    name: RuleName,
    category: RuleCategory,
    severity: Severity,
    description: &'static str,
    fixability: Fixability,
}

impl RuleMeta {
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
        }
    }

    pub const fn name(&self) -> RuleName {
        self.name
    }

    pub const fn category(&self) -> RuleCategory {
        self.category
    }

    pub const fn severity(&self) -> Severity {
        self.severity
    }

    pub const fn description(&self) -> &'static str {
        self.description
    }

    pub const fn fixability(&self) -> Fixability {
        self.fixability
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
        const FIXABLE: bool = SAMPLE.fixability().is_fixable();
        assert_eq!(NAME, "zero-divisor");
        assert_eq!(CATEGORY, "suspicious");
        assert!(!FIXABLE);
        assert_eq!(SAMPLE.severity(), Severity::Warning);
    }
}
