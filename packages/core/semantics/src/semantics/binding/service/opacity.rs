//! Which regions are beyond static reach.
//!
//! Two different questions hide behind the word "opaque", and this module
//! keeps them apart because they need different answers:
//!
//! * *May the walk descend?* — answered by refusing to enter quoted data and
//!   reader-controlled text, exactly where `lexical_scope`'s reference query
//!   also refuses. Descent must match the query or the table stops being an
//!   inversion of it.
//! * *May the value layer trust what it found?* — answered by
//!   [`ScopeOpacity`], which is recorded without stopping the walk. An unknown
//!   head might be a macro whose expansion rebinds or reassigns, so every
//!   binding in scope is flagged even though the walk still descends and
//!   attributes the references it can see.
//!
//! Marking without stopping is the only combination that satisfies both: a
//! table that stopped at `(print x)` would attribute no references at all,
//! since Common Lisp has no registry of ordinary function heads.

use crate::domain::common_lisp::CommonLispOperator;

use super::super::policy::{
    is_pure_standard_function, is_standard_control_form, is_standard_declaration_identifier,
};

/// Whether `head` names a form whose semantics this layer knows.
///
/// Four sources count, each transparent for a different reason.
/// [`CommonLispOperator`] registers the special and definition forms —
/// everything that binds. The standard-function table covers ordinary calls: a
/// function receives its arguments' values and has no access to the caller's
/// lexical environment, so it provably cannot reassign a binding. The
/// control-form table covers `if`, `progn`, `setq` and their kin, which *can*
/// reach a binding but evaluate their subforms where they are written, leaving
/// any assignment visible for the assignment collector. The declaration table
/// covers what sits inside `(declare …)`, which is not evaluated at all.
///
/// What remains unregistered is the case that actually matters: an unknown
/// head might be a macro, and a macro can expand into `(setq x …)` with
/// nothing in the source to show for it. Flagging only those keeps the rule
/// sound while leaving ordinary code analysable — treating `(print x)` as
/// opaque too would be sound as well, and would prove nothing about any real
/// file.
pub(super) fn head_has_registered_semantics(head: &str) -> bool {
    CommonLispOperator::from_head(head).is_some()
        || is_pure_standard_function(head)
        || is_standard_control_form(head)
        || is_standard_declaration_identifier(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_forms_are_registered() {
        assert!(head_has_registered_semantics("let"));
        assert!(head_has_registered_semantics("LET*"));
        assert!(head_has_registered_semantics("defun"));
    }

    #[test]
    fn standard_functions_are_registered_because_they_cannot_assign() {
        assert!(head_has_registered_semantics("/"));
        assert!(head_has_registered_semantics("length"));
    }

    #[test]
    fn declaration_specifiers_are_registered_because_they_are_never_evaluated() {
        // `(declare (ignore x))` is walked as if `(ignore x)` were a call.
        // Leaving it unregistered made the enclosing scope opaque for a form
        // that has no run-time semantics at all.
        assert!(head_has_registered_semantics("ignore"));
        assert!(head_has_registered_semantics("OPTIMIZE"));
        assert!(head_has_registered_semantics("speed"));
    }

    #[test]
    fn an_unknown_head_stays_unregistered_because_it_might_be_a_macro() {
        // `my-with-thing` could expand into `(setq x …)`; nothing in the
        // source rules that out. `with-open-file` is a standard *macro* whose
        // expansion this layer does not model, so it stays opaque too.
        assert!(!head_has_registered_semantics("my-with-thing"));
        assert!(!head_has_registered_semantics("app:helper"));
    }
}
