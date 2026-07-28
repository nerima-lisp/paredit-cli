//! The stable identifier a lint rule is selected and reported by.

use std::fmt;

/// A lint rule's stable identifier, e.g. `zero-divisor`.
///
/// The same string appears in `--rule`/`--exclude` selectors, in the `rule`
/// field of every finding, and as the rule's own `inspect` subcommand name, so
/// it is a domain identity rather than an incidental label. Wrapping it keeps
/// a rule name from being confused with a category name, a head spelling, or a
/// message — all of which are also `&'static str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleName(&'static str);

impl RuleName {
    /// Wraps a rule identifier. `const` so the registry can build its metadata
    /// at compile time.
    #[must_use]
    pub const fn new(value: &'static str) -> Self {
        assert!(!value.is_empty(), "a lint rule name cannot be empty");
        Self(value)
    }

    /// The identifier as it appears in selectors and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for RuleName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl PartialEq<str> for RuleName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_identifier() {
        assert_eq!(RuleName::new("zero-divisor").as_str(), "zero-divisor");
    }

    #[test]
    fn displays_as_the_bare_identifier() {
        assert_eq!(RuleName::new("if-not").to_string(), "if-not");
    }
}
