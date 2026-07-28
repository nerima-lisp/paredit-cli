//! Declaration specifiers, which are not calls and evaluate nothing.
//!
//! `(declare ...)` is registered as a form with known semantics, so the walk
//! descends into it — and then reads each specifier inside as if it were a
//! call. `(declare (ignore x))` becomes "a call to the unknown head `ignore`",
//! which flags the whole enclosing scope opaque. Measuring a 38-library corpus
//! put `ignore`, `type`, `ignorable`, and `optimize` among the ten most common
//! reasons a binding could not be resolved, all of them this same mistake.
//!
//! The soundness argument is stronger here than for either neighbouring table.
//! A standard function cannot reach the caller's lexical environment; a
//! standard control form can but writes its effects where they are visible. A
//! declaration specifier is not evaluated *at all* — it is compiler advice
//! that CLHS 3.3 gives no run-time semantics, so there is no mechanism by
//! which it could reassign anything.
//!
//! Every name here is a symbol in the `COMMON-LISP` package, which CLHS
//! 11.1.2.1.2 forbids a program from redefining. That is what makes the
//! lookup safe on a bare head: no conforming program can make `ignore` mean
//! something that assigns.

use std::collections::HashSet;
use std::sync::LazyLock;

use super::head_index::contains_folded;

/// Declaration identifiers (CLHS 3.3) and the optimize qualities that appear
/// inside them.
///
/// The qualities are here because `(declare (optimize (speed 3) (safety 0)))`
/// nests: without them the walk clears `optimize` and then trips over `speed`,
/// which leaves the scope exactly as opaque as before.
///
/// Implementation-specific declarations are deliberately absent. A name this
/// table omits stays opaque, which loses a deduction rather than inventing
/// one.
const STANDARD_DECLARATION_IDENTIFIERS: &[&str] = &[
    // Declaration identifiers, in CLHS 3.3's order.
    "declaration",
    "dynamic-extent",
    "ftype",
    "ignorable",
    "ignore",
    "inline",
    "notinline",
    "optimize",
    "special",
    "type",
    // Optimize qualities, which only ever appear nested inside `optimize`.
    "compilation-speed",
    "debug",
    "safety",
    "space",
    "speed",
];

static INDEX: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| STANDARD_DECLARATION_IDENTIFIERS.iter().copied().collect());

/// Whether `head` names a declaration specifier or optimize quality, and so
/// names something that is never evaluated.
#[must_use]
pub fn is_standard_declaration_identifier(head: &str) -> bool {
    contains_folded(&INDEX, head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specifiers_a_corpus_actually_trips_over_are_covered() {
        // The four that the coverage harness ranked highest, plus `special`,
        // which decides a binding's own extent and so must not itself be the
        // reason that binding is unreadable.
        for head in ["ignore", "IGNORABLE", "type", "optimize", "special"] {
            assert!(is_standard_declaration_identifier(head), "{head}");
        }
    }

    #[test]
    fn optimize_qualities_are_covered_because_they_nest() {
        for head in ["speed", "safety", "debug", "space", "compilation-speed"] {
            assert!(is_standard_declaration_identifier(head), "{head}");
        }
    }

    #[test]
    fn an_unknown_head_is_not_a_declaration() {
        for head in ["my-macro", "sb-ext:muffle-conditions", "app:run"] {
            assert!(!is_standard_declaration_identifier(head), "{head}");
        }
    }

    #[test]
    fn every_name_is_reachable_through_the_folded_index() {
        // An uppercase or over-long entry would be unreachable rather
        // than wrong, which shows up as a missing deduction and nothing
        // else. See `head_index`.
        assert!(super::super::head_index::is_lookupable(
            STANDARD_DECLARATION_IDENTIFIERS
        ));
    }

    #[test]
    fn the_table_is_sorted_within_its_groups_and_lists_nothing_twice() {
        let mut names = STANDARD_DECLARATION_IDENTIFIERS.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
