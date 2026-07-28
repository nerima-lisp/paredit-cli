//! The long-form documentation `--explain <rule>` prints.
//!
//! [`super::RuleMeta::description`] is one line, because it has to fit a column
//! of `--list-rules`. One line is enough to *recognize* a rule and nowhere near
//! enough to *act* on one: an agent that reads "a (the t form) type
//! declaration, which is vacuous" still has to guess what to write instead, and
//! whether rewriting is safe.
//!
//! So this carries the three things a one-liner cannot: why the pattern is
//! worth changing, a minimal example of each side, and — for the rules where
//! there is one — the case that looks like a hit but is deliberately left
//! alone. That last field is the one that saves the round trip, because "why
//! didn't it fire here?" is otherwise unanswerable from the outside.

/// A worked example pair: the code a rule reports, and what to write instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleExample {
    bad: &'static str,
    good: &'static str,
}

impl RuleExample {
    #[must_use]
    pub const fn new(bad: &'static str, good: &'static str) -> Self {
        assert!(!bad.is_empty(), "a rule example needs the reported form");
        assert!(
            !good.is_empty(),
            "a rule example needs the form to write instead"
        );
        Self { bad, good }
    }

    /// The form the rule reports.
    #[must_use]
    pub const fn bad(self) -> &'static str {
        self.bad
    }

    /// The form to write instead. For a fixable rule this is what `--fix`
    /// produces.
    #[must_use]
    pub const fn good(self) -> &'static str {
        self.good
    }
}

/// Everything `--explain <rule>` prints beyond the metadata.
///
/// Every field is optional-by-emptiness rather than `Option`, so the struct
/// stays `const`-constructible from a rule's `META` without a builder for each
/// combination. A rule that supplies none of them still gets a useful
/// `--explain`: the renderer falls back to the metadata it always has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleExplanation {
    rationale: &'static str,
    example: Option<RuleExample>,
    caveat: &'static str,
}

impl RuleExplanation {
    /// The minimum: why the pattern is worth changing.
    #[must_use]
    pub const fn new(rationale: &'static str) -> Self {
        assert!(
            !rationale.is_empty(),
            "an explanation's whole purpose is the rationale"
        );
        Self {
            rationale,
            example: None,
            caveat: "",
        }
    }

    /// Adds the before/after pair.
    #[must_use]
    pub const fn with_example(mut self, bad: &'static str, good: &'static str) -> Self {
        self.example = Some(RuleExample::new(bad, good));
        self
    }

    /// Adds the "looks like a hit, deliberately not reported" note — the field
    /// that answers "why didn't it fire here?" without a round trip.
    #[must_use]
    pub const fn with_caveat(mut self, caveat: &'static str) -> Self {
        assert!(!caveat.is_empty(), "an empty caveat is no caveat");
        self.caveat = caveat;
        self
    }

    #[must_use]
    pub const fn rationale(self) -> &'static str {
        self.rationale
    }

    #[must_use]
    pub const fn example(self) -> Option<RuleExample> {
        self.example
    }

    /// The caveat, or `""` when the rule declared none.
    #[must_use]
    pub const fn caveat(self) -> &'static str {
        self.caveat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: RuleExplanation = RuleExplanation::new(
        "`t` is the type every object has, so the assertion constrains nothing.",
    )
    .with_example("(the t (compute))", "(compute)")
    .with_caveat("A compound specifier such as `(integer 0 9)` is left alone.");

    #[test]
    fn builds_every_field_in_a_const_context() {
        const RATIONALE: &str = SAMPLE.rationale();
        assert!(RATIONALE.starts_with("`t` is"));
        assert_eq!(
            SAMPLE.example().expect("example").bad(),
            "(the t (compute))"
        );
        assert_eq!(SAMPLE.example().expect("example").good(), "(compute)");
        assert!(SAMPLE.caveat().contains("compound"));
    }

    #[test]
    fn a_bare_explanation_has_no_example_and_an_empty_caveat() {
        const BARE: RuleExplanation = RuleExplanation::new("because.");
        assert_eq!(BARE.example(), None);
        assert_eq!(BARE.caveat(), "");
    }
}
