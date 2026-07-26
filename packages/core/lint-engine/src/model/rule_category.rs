//! The coarse grouping a lint rule belongs to, used by `--category`.

/// The bug family a rule detects. This is the enumeration behind the
/// `--category` selector and the `category` column of `--list-rules`; keeping
/// it closed means a typo in a rule's metadata is a compile error rather than
/// a category that silently matches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleCategory {
    /// A call with an argument count the operator cannot accept.
    Arity,
    /// Code that can never run or whose result is discarded.
    DeadCode,
    /// The same key, place, test, or name given twice.
    Duplicate,
    /// A form whose shape does not match what the operator requires.
    Malformed,
    /// Well-formed code whose meaning is probably not what was intended.
    Suspicious,
}

impl RuleCategory {
    /// Every category, in the order `--list-rules` and `CATEGORIES` present
    /// them (alphabetical by wire name).
    pub const ALL: [Self; 5] = [
        Self::Arity,
        Self::DeadCode,
        Self::Duplicate,
        Self::Malformed,
        Self::Suspicious,
    ];

    /// The category's wire name, as accepted by `--category` and printed in
    /// reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Arity => "arity",
            Self::DeadCode => "dead-code",
            Self::Duplicate => "duplicate",
            Self::Malformed => "malformed",
            Self::Suspicious => "suspicious",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_are_unique_and_sorted() {
        let names: Vec<&str> = RuleCategory::ALL.iter().map(|c| c.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }
}
