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

/// Whether `head` names a standard form whose subforms are evaluated in place,
/// so that any assignment it performs is visible in the source.
pub fn is_standard_control_form(head: &str) -> bool {
    STANDARD_CONTROL_FORMS
        .iter()
        .any(|name| name.eq_ignore_ascii_case(head))
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
    fn no_name_is_listed_twice() {
        let mut names = STANDARD_CONTROL_FORMS.to_vec();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
