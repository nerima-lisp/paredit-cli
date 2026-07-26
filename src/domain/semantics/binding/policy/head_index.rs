//! Case-insensitive head lookup that does not scan and does not allocate.
//!
//! The tables beside this module are consulted once per list in the file, and
//! the expensive case is the one that matters most: an *unregistered* head
//! falls through every table, so a linear scan costs the sum of all their
//! lengths. Growing the standard-function table from 150 names to 290 after
//! measuring a real corpus made the dense lint benchmark 64% slower for
//! exactly that reason — synthetic dense input is mostly invented heads.
//!
//! Hashing needs a case-folded key, and Common Lisp reads symbols
//! case-insensitively, so the obvious `head.to_ascii_lowercase()` would put a
//! heap allocation on that same per-list path. Folding into a stack buffer
//! avoids it: no head worth registering is long, and one that overruns the
//! buffer cannot be in a table whose entries all fit.

use std::collections::HashSet;

/// The longest head the fold buffer holds.
///
/// Every table asserts its own names fit, so a head longer than this is
/// provably absent rather than merely unhandled.
const MAX_FOLDED_HEAD: usize = 64;

/// Whether `index` holds `head`, compared the way the reader would.
///
/// `index` must hold lowercase names; the tables' own tests pin that.
pub(super) fn contains_folded(index: &HashSet<&'static str>, head: &str) -> bool {
    let bytes = head.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_FOLDED_HEAD {
        return false;
    }

    let mut buffer = [0u8; MAX_FOLDED_HEAD];
    for (slot, byte) in buffer.iter_mut().zip(bytes) {
        *slot = byte.to_ascii_lowercase();
    }

    // Lowercasing only ASCII bytes cannot break UTF-8: every byte it changes
    // is a single-byte code point, and multi-byte sequences pass through
    // untouched. The check is kept rather than asserted away because it costs
    // nothing next to the hash that follows.
    std::str::from_utf8(&buffer[..bytes.len()]).is_ok_and(|folded| index.contains(folded))
}

/// Whether every name in `table` is lowercase and short enough to be found.
///
/// A shared assertion for the tables' unit tests: a name that fails either
/// test is silently unreachable through [`contains_folded`], which is the kind
/// of bug that shows up as a missing deduction rather than a failure.
#[cfg(test)]
pub(super) fn is_lookupable(table: &[&'static str]) -> bool {
    table.iter().all(|name| {
        !name.is_empty()
            && name.len() <= MAX_FOLDED_HEAD
            && name
                .chars()
                .all(|character| !character.is_ascii_uppercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> HashSet<&'static str> {
        ["let", "make-instance", "="].into_iter().collect()
    }

    #[test]
    fn a_head_is_found_whatever_case_it_is_written_in() {
        for head in ["let", "LET", "Let", "MAKE-INSTANCE"] {
            assert!(contains_folded(&index(), head), "{head}");
        }
    }

    #[test]
    fn an_absent_head_is_not_found() {
        for head in ["lets", "le", "my-macro", ""] {
            assert!(!contains_folded(&index(), head), "{head}");
        }
    }

    #[test]
    fn a_head_too_long_to_fold_is_absent_rather_than_truncated() {
        // Truncating would make a 100-character head match a 64-character
        // table entry. No table entry is that long, so "absent" is also the
        // right answer.
        let long = "a".repeat(MAX_FOLDED_HEAD + 1);
        assert!(!contains_folded(&index(), &long));
    }

    #[test]
    fn a_non_ascii_head_folds_without_corrupting_its_bytes() {
        let index: HashSet<&'static str> = ["λ-form"].into_iter().collect();
        assert!(contains_folded(&index, "λ-form"));
        assert!(contains_folded(&index, "λ-FORM"));
    }
}
