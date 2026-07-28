//! Orthogonal properties of a rule, independent of its category.
//!
//! [`super::RuleCategory`] answers "what kind of bug does this find?" and is a
//! partition: every rule has exactly one. That leaves no room for the facts a
//! caller actually filters on — "is this rule stable?", "is its fix
//! behaviour-preserving?", "does it cost a semantic table?" — because those are
//! *orthogonal* to the bug family and to each other. A `security` rule can be
//! experimental; a `suspicious` rule can be pedantic; either can be both.
//!
//! Hence a set rather than a second enum. The set is a `u16` bitmask so it can
//! be built and queried in a `const` context, which is what lets
//! [`super::RuleMeta`] stay `const` and the catalogue stay compile-time
//! derived.

/// One orthogonal property of a rule.
///
/// Deliberately *not* including "fixable": that is [`super::Fixability`], which
/// is paired with a guard test asserting that exactly the fixable rules emit a
/// fix. Duplicating it as a tag would create two facts that can drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleTag {
    /// The rule is new or its heuristic is still being tuned. Excluded from
    /// every preset except `all`; `--experimental` opts back in.
    Experimental,
    /// Correct, but opinionated enough to be noise on a codebase that has not
    /// adopted the convention. Included only by the `pedantic` preset.
    Pedantic,
    /// The rule's automatic fix can change runtime behaviour rather than only
    /// spelling. `--fix` still applies it; `--no-destructive-fixes` does not.
    ///
    /// The overwhelming majority of fixes are equivalences (`(the t x)` *is*
    /// `x`). The few that are not — replacing a linear search with a hash
    /// lookup, say — must be visibly different from those.
    Destructive,
    /// The rule consults the binding, value, or type table. Those are built
    /// once per file on first use, so a run whose active rules carry none of
    /// this tag never builds them; `--timings` shows why the ones that do cost
    /// more.
    Semantic,
    /// The rule's subject is layout or naming, not behaviour. Nothing it
    /// reports can change what the program computes.
    Style,
    /// The rule needs more than the file it is looking at, so it only reports
    /// under the project-wide pass.
    CrossFile,
}

impl RuleTag {
    /// Every tag, in the order `--list-rules` and `--explain` print them.
    pub const ALL: [Self; 6] = [
        Self::Experimental,
        Self::Pedantic,
        Self::Destructive,
        Self::Semantic,
        Self::Style,
        Self::CrossFile,
    ];

    /// The tag's wire name, as accepted by `--tag` and printed in reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Experimental => "experimental",
            Self::Pedantic => "pedantic",
            Self::Destructive => "destructive",
            Self::Semantic => "semantic",
            Self::Style => "style",
            Self::CrossFile => "cross-file",
        }
    }

    /// The wire name back to a tag, for `--tag`.
    ///
    /// Not `FromStr`: that trait's `Err` would have to be a type nobody wants,
    /// and every caller here already has a better message to print than a
    /// parse error.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tag| tag.as_str() == value)
    }

    const fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

/// The set of tags one rule carries.
///
/// A bitmask so the whole thing is `const`-constructible and `const`-queryable:
/// the catalogue's derived arrays (`EXPERIMENTAL_RULES`, the preset membership
/// tests) are computed at compile time from the registry, exactly like
/// `FIXABLE_RULES` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuleTags(u16);

impl RuleTags {
    /// The default: a stable rule with no special properties.
    pub const NONE: Self = Self(0);

    /// Builds a set from a slice, which is the spelling a rule's `META` uses.
    #[must_use]
    pub const fn of(tags: &[RuleTag]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < tags.len() {
            bits |= tags[index].bit();
            index += 1;
        }
        Self(bits)
    }

    #[must_use]
    pub const fn contains(self, tag: RuleTag) -> bool {
        self.0 & tag.bit() != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The tags in [`RuleTag::ALL`] order, for rendering.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        RuleTag::ALL
            .into_iter()
            .filter(|tag| self.contains(*tag))
            .map(RuleTag::as_str)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_set_contains_nothing() {
        assert!(RuleTags::NONE.is_empty());
        for tag in RuleTag::ALL {
            assert!(!RuleTags::NONE.contains(tag));
        }
    }

    #[test]
    fn a_set_contains_exactly_what_it_was_built_from() {
        const TAGS: RuleTags = RuleTags::of(&[RuleTag::Pedantic, RuleTag::Style]);
        // In a `const` block, so the whole query really is answered at compile
        // time — which is the property the catalogue's derived arrays rely on.
        const { assert!(TAGS.contains(RuleTag::Pedantic)) };
        assert!(TAGS.contains(RuleTag::Style));
        assert!(!TAGS.contains(RuleTag::Experimental));
        assert!(!TAGS.contains(RuleTag::Destructive));
    }

    #[test]
    fn names_follow_the_declared_tag_order_not_the_build_order() {
        let tags = RuleTags::of(&[RuleTag::Style, RuleTag::Experimental]);
        assert_eq!(tags.names(), vec!["experimental", "style"]);
    }

    #[test]
    fn wire_names_round_trip() {
        for tag in RuleTag::ALL {
            assert_eq!(RuleTag::parse(tag.as_str()), Some(tag));
        }
        assert_eq!(RuleTag::parse("no-such-tag"), None);
    }

    #[test]
    fn every_tag_gets_its_own_bit() {
        // Guards the `1 << self as u16` encoding: adding a seventh tag without
        // widening the mask would silently alias two tags together.
        let mut seen = 0u16;
        for tag in RuleTag::ALL {
            let bits = RuleTags::of(&[tag]).0;
            assert!(seen & bits == 0, "{} aliases an earlier tag", tag.as_str());
            seen |= bits;
        }
    }
}
