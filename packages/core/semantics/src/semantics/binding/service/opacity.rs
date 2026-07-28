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

use paredit_core_syntax::common_lisp::CommonLispOperator;
use paredit_core_syntax::dialect::Dialect;
use paredit_core_syntax::emacs_lisp::EmacsLispOperator;
use paredit_core_syntax::scheme::scheme_head_has_registered_semantics;

use super::super::policy::{
    is_pure_standard_emacs_lisp_function, is_pure_standard_function,
    is_pure_standard_scheme_procedure, is_standard_control_form,
    is_standard_declaration_identifier,
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
pub(super) fn head_has_registered_semantics(dialect: Dialect, head: &str) -> bool {
    match dialect {
        // Scheme's own table is the only one that means anything here.
        // Falling through to the Common Lisp tables would register `list` and
        // `not` -- the names happen to coincide -- while leaving `vector-ref`
        // and `string=?` opaque, which is an arbitrary line rather than a
        // sound one.
        Dialect::Scheme | Dialect::Racket => {
            scheme_head_has_registered_semantics(head) || is_pure_standard_scheme_procedure(head)
        }
        // Emacs Lisp has no second table of "control forms": its operator
        // table already holds `progn`, `unwind-protect`, `save-excursion` and
        // the rest, marked by `evaluates_subforms_in_place`.
        Dialect::EmacsLisp => {
            EmacsLispOperator::from_head(head).is_some()
                || is_pure_standard_emacs_lisp_function(head)
        }
        _ => {
            CommonLispOperator::from_head(head).is_some()
                || is_pure_standard_function(head)
                || is_standard_control_form(head)
                || is_standard_declaration_identifier(head)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_forms_are_registered() {
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "let"));
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "LET*"));
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "defun"));
    }

    #[test]
    fn emacs_lisp_registers_its_own_vocabulary_and_not_common_lisp_case_folding() {
        assert!(head_has_registered_semantics(Dialect::EmacsLisp, "if-let*"));
        assert!(head_has_registered_semantics(Dialect::EmacsLisp, "cl-flet"));
        assert!(head_has_registered_semantics(
            Dialect::EmacsLisp,
            "with-temp-buffer"
        ));
        assert!(head_has_registered_semantics(Dialect::EmacsLisp, "length"));
        // `LET*` is a symbol a `.el` file may define; it is not the special
        // form, so a call to it must stay opaque.
        assert!(!head_has_registered_semantics(Dialect::EmacsLisp, "LET*"));
    }

    #[test]
    fn standard_functions_are_registered_because_they_cannot_assign() {
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "/"));
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "length"));
    }

    #[test]
    fn declaration_specifiers_are_registered_because_they_are_never_evaluated() {
        // `(declare (ignore x))` is walked as if `(ignore x)` were a call.
        // Leaving it unregistered made the enclosing scope opaque for a form
        // that has no run-time semantics at all.
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "ignore"));
        assert!(head_has_registered_semantics(
            Dialect::CommonLisp,
            "OPTIMIZE"
        ));
        assert!(head_has_registered_semantics(Dialect::CommonLisp, "speed"));
    }

    #[test]
    fn an_unknown_head_stays_unregistered_because_it_might_be_a_macro() {
        // `my-with-thing` could expand into `(setq x …)`; nothing in the
        // source rules that out. `with-open-file` is a standard *macro* whose
        // expansion this layer does not model, so it stays opaque too.
        assert!(!head_has_registered_semantics(
            Dialect::CommonLisp,
            "my-with-thing"
        ));
        assert!(!head_has_registered_semantics(
            Dialect::CommonLisp,
            "app:helper"
        ));
    }
}
