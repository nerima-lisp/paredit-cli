//! Whether a rule can synthesize an automatic rewrite for its findings.

/// Whether `inspect lint --fix` can repair a rule's findings.
///
/// A `bool` field would leave "fixable" and "produces a fix" as two facts that
/// can drift apart; making it an enum on [`super::RuleMeta`] pairs with a guard
/// test asserting that exactly the [`Fixability::Fixable`] rules ever emit a
/// [`super::RuleFix`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixability {
    /// The repair is mechanical, so the rule attaches a fix to its findings.
    Fixable,
    /// The repair depends on intent a machine cannot infer; diagnostic only.
    ReportOnly,
}

impl Fixability {
    pub const fn is_fixable(self) -> bool {
        matches!(self, Self::Fixable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fixable_reports_as_fixable() {
        assert!(Fixability::Fixable.is_fixable());
        assert!(!Fixability::ReportOnly.is_fixable());
    }
}
