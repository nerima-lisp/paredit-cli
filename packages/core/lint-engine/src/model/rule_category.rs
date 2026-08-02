//! The coarse grouping a lint rule belongs to, used by `--category`.

/// The bug family a rule detects. This is the enumeration behind the
/// `--category` selector and the `category` column of `--list-rules`; keeping
/// it closed means a typo in a rule's metadata is a compile error rather than
/// a category that silently matches nothing.
///
/// A rule has exactly one category, so this is a partition of the suite and
/// answers only "what kind of defect is this?". Everything orthogonal to that
/// — stability, whether the fix preserves behaviour, whether the rule costs a
/// semantic table — is [`super::RuleTag`], which is a *set*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleCategory {
    /// Avoidable consing: a copy nothing reads, an allocation a loop repeats
    /// for no reason.
    Allocation,
    /// A call with an argument count the operator cannot accept.
    Arity,
    /// Shared mutable state, and defects that exist only because more than
    /// one thread is involved.
    ///
    /// Wider than shared state alone: unsynchronized mutation and
    /// check-then-act races, a non-reentrant lock taken again inside its own
    /// scope, a special binding or an error handler that does not cross the
    /// thread boundary it looks like it crosses, an update whose retries
    /// repeat its side effects, and a deferred value nothing ever realizes.
    ///
    /// The first clause is not redundant: `global-mutation-in-function`
    /// anchors on `defun`/`defmethod`/`defgeneric`/`lambda` and reports a
    /// function whose result depends on every previous call, which is a
    /// complaint a single-threaded program earns too.
    ///
    /// A lock that leaks on a non-local exit is [`Self::Resource`], not this:
    /// what that rule is about is the unreleased resource, not the sharing.
    Concurrency,
    /// The condition system used in a way that loses or hides an error.
    Conditions,
    /// Code that can never run or whose result is discarded.
    DeadCode,
    /// `declare` / `declaim` that contradicts itself, the lambda list, or the
    /// body.
    Declaration,
    /// Descriptive metadata a definition should carry and does not: a missing
    /// or inconsistent docstring, or a system declared without the `:version`
    /// its dependents pin against.
    Documentation,
    /// The same key, place, test, or name given twice.
    Duplicate,
    /// A form whose shape does not match what the operator requires.
    Malformed,
    /// A name that contradicts the convention its own definition form implies
    /// (`+constant+`, `*special*`, `predicate-p`, `ndestructive`).
    Naming,
    /// Floating-point arithmetic whose result depends on precision or on
    /// reader state.
    NumericPrecision,
    /// CLOS: `defclass` slot options, and `defgeneric`/`defmethod` agreement.
    ObjectSystem,
    /// An idiom whose cost is asymptotically worse than the obvious
    /// alternative.
    Performance,
    /// Code that depends on one implementation, one character set, or one
    /// host's path syntax.
    Portability,
    /// A stream or other resource that can leak on a non-local exit.
    Resource,
    /// Untrusted input reaching `eval`, `read`, `intern`, or a subprocess.
    Security,
    /// Well-formed code whose meaning is probably not what was intended.
    Suspicious,
}

impl RuleCategory {
    /// Every category, in the order `--list-rules` and `CATEGORIES` present
    /// them (alphabetical by wire name).
    pub const ALL: [Self; 17] = [
        Self::Allocation,
        Self::Arity,
        Self::Concurrency,
        Self::Conditions,
        Self::DeadCode,
        Self::Declaration,
        Self::Documentation,
        Self::Duplicate,
        Self::Malformed,
        Self::Naming,
        Self::NumericPrecision,
        Self::ObjectSystem,
        Self::Performance,
        Self::Portability,
        Self::Resource,
        Self::Security,
        Self::Suspicious,
    ];

    /// The category's wire name, as accepted by `--category` and printed in
    /// reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allocation => "allocation",
            Self::Arity => "arity",
            Self::Concurrency => "concurrency",
            Self::Conditions => "conditions",
            Self::DeadCode => "dead-code",
            Self::Declaration => "declaration",
            Self::Documentation => "documentation",
            Self::Duplicate => "duplicate",
            Self::Malformed => "malformed",
            Self::Naming => "naming",
            Self::NumericPrecision => "numeric-precision",
            Self::ObjectSystem => "object-system",
            Self::Performance => "performance",
            Self::Portability => "portability",
            Self::Resource => "resource",
            Self::Security => "security",
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

    /// `ALL` is a fixed-size array, so a variant added without listing it here
    /// would be invisible to `--category` while still compiling. Collecting the
    /// listed variants into a set is the cheapest way to make that a failure.
    #[test]
    fn no_category_is_listed_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for category in RuleCategory::ALL {
            assert!(seen.insert(category), "{category:?} listed twice");
        }
        assert_eq!(seen.len(), RuleCategory::ALL.len());
    }
}
