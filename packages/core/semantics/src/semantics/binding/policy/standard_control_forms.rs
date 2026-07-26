//! Standard forms that evaluate their subforms in place.
//!
//! [`super::is_pure_standard_function`] covers calls; this covers the rest of
//! what a body normally contains. `CommonLispOperator` registers only the
//! forms that *bind*, so `if`, `progn`, `cond`, and even `setq` are unknown to
//! it — which would make almost every `let` body opaque and leave the value
//! layer with nothing to say.
//!
//! These forms are safe to see through for a different reason than functions
//! are. A function cannot reach the caller's lexical environment at all. A
//! control form can — `(if c (setq x 1) …)` assigns — but it evaluates its
//! subforms *where they are written*, so any assignment inside is visible in
//! the source and already recorded by the assignment collector. What stays
//! opaque is the case neither argument covers: an unknown head, which may be a
//! macro that expands into an assignment leaving nothing in the source at all.

use std::collections::HashSet;
use std::sync::LazyLock;

use super::head_index::contains_folded;

/// Standard special forms and macros whose subforms are evaluated in place.
///
/// The assignment operators belong here too: `(setq x 1)` is not a reason to
/// distrust the *scope*, because the assignment it performs is exactly what
/// [`super::assignment_forms`] records. Marking the scope opaque as well would
/// punish every other binding in it for one visible assignment.
///
/// Binding forms are deliberately absent — `CommonLispOperator` already knows
/// them, and this table exists only to fill that gap.
const STANDARD_CONTROL_FORMS: &[&str] = &[
    // Conditionals and sequencing.
    "if",
    "when",
    "unless",
    "cond",
    "case",
    "ccase",
    "ecase",
    "typecase",
    "ctypecase",
    "etypecase",
    "progn",
    "prog1",
    "prog2",
    "and",
    "or",
    // Multiple values. Each evaluates its subforms where they are written and
    // only rearranges the values that come back.
    //
    // `multiple-value-setq` is the exception that proves the rule: it *does*
    // write, which is why `assignment_forms` lists it. It is transparent for
    // the same reason `setq` is — the names it writes are spelled out in the
    // source, so the collector already has them.
    "multiple-value-call",
    "multiple-value-list",
    "multiple-value-prog1",
    "multiple-value-setq",
    "nth-value",
    // Establishes a declaration scope around a body written out in place.
    "locally",
    // Binds the standard printer and reader control variables — all of them
    // special, none of them anything a lexical binding here could be — and
    // evaluates its body in place. `progv` is deliberately absent: it binds a
    // *computed* list of names, so which variables it shadows cannot be read
    // off the source at all.
    "with-standard-io-syntax",
    // Non-local exit, all of whose bodies are written out.
    "block",
    "return",
    "return-from",
    "tagbody",
    "go",
    "catch",
    "throw",
    "unwind-protect",
    // Declarations and quotation, which evaluate nothing new.
    "the",
    "quote",
    "declare",
    "declaim",
    "eval-when",
    // Error signalling whose body is written out where it stands.
    //
    // `ignore-errors` is `(handler-case (progn body) (error (c) …))`: the body
    // is evaluated in place, and nothing in the expansion reaches a binding
    // the body did not already reach.
    //
    // `assert` and `check-type` are subtler, and are transparent only because
    // `assignment_forms` lists them. Both can *write* to a place — `assert`
    // through the restart for each place it names, `check-type` through
    // `store-value` on its first argument. Registering them here without that
    // entry would let the value layer propagate through a binding the program
    // is allowed to replace. With it, the write is recorded exactly like a
    // `setf`, and the rest of the scope stops paying for the uncertainty.
    "ignore-errors",
    "assert",
    "check-type",
    // Assignment, whose effect the assignment collector already records.
    "setq",
    "setf",
    "psetq",
    "psetf",
    "incf",
    "decf",
    "push",
    "pop",
    "pushnew",
    "rotatef",
    "shiftf",
];

static INDEX: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| STANDARD_CONTROL_FORMS.iter().copied().collect());

/// Whether `head` names a standard form whose subforms are evaluated in place,
/// so that any assignment it performs is visible in the source.
pub fn is_standard_control_form(head: &str) -> bool {
    contains_folded(&INDEX, head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conditionals_and_sequencing_are_transparent() {
        for head in ["if", "WHEN", "cond", "progn", "and"] {
            assert!(is_standard_control_form(head), "{head}");
        }
    }

    #[test]
    fn assignment_forms_are_transparent_because_their_effect_is_recorded() {
        for head in ["setq", "setf", "incf", "push", "rotatef"] {
            assert!(is_standard_control_form(head), "{head}");
        }
    }

    #[test]
    fn an_unknown_head_is_not_a_control_form() {
        for head in ["my-macro", "with-open-file", "app:run"] {
            assert!(!is_standard_control_form(head), "{head}");
        }
    }

    #[test]
    fn every_name_is_reachable_through_the_folded_index() {
        // An uppercase or over-long entry would be unreachable rather
        // than wrong, which shows up as a missing deduction and nothing
        // else. See `head_index`.
        assert!(super::super::head_index::is_lookupable(
            STANDARD_CONTROL_FORMS
        ));
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut names = STANDARD_CONTROL_FORMS.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
