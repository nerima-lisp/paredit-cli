//! Which dialects the type layer runs for.

use paredit_core_syntax::dialect::Dialect;

/// Whether type inference is enabled for `dialect`.
///
/// Common Lisp's coverage is CLHS-specific: the declaration forms (`the`,
/// `check-type`, `declare`) and the standard-function return types borrow
/// directly from the standard. Emacs Lisp is enabled too, on its own,
/// narrower declaration sources — `cl-defstruct` slot `:type` options and
/// `defcustom`'s `:type` — rather than by reusing any of Common Lisp's; see
/// `service::declarations` for the Common Lisp sources and
/// `service::emacs_lisp_declarations` for the Emacs Lisp ones. Every other
/// dialect gets an empty table, so every rule that consumes types stays
/// silent there.
#[must_use]
pub const fn supports_type_inference(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_and_emacs_lisp_are_enabled() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            assert!(supports_type_inference(dialect), "{dialect:?}");
        }
        for dialect in [
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            assert!(!supports_type_inference(dialect), "{dialect:?}");
        }
    }
}
