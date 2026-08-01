//! Which dialects the type layer runs for.

use paredit_core_syntax::dialect::Dialect;

/// Whether type inference is enabled for `dialect`.
///
/// Common Lisp's coverage is CLHS-specific: the declaration forms (`the`,
/// `check-type`, `declare`) and the standard-function return types borrow
/// directly from the standard. Every other enabled dialect is on its own,
/// narrower declaration sources rather than by reusing Common Lisp's:
///
/// * Emacs Lisp — `cl-defstruct` slot `:type` options and `defcustom`'s
///   `:type` (`service::emacs_lisp_declarations`).
/// * Scheme and Racket — `define-record-type` predicates, plus Racket's own
///   Typed Racket `(: name (-> …))` function annotations
///   (`service::scheme_declarations`).
///
/// Common Lisp's own sources live in `service::declarations`. Every other
/// dialect gets an empty table, so every rule that consumes types stays
/// silent there.
#[must_use]
pub const fn supports_type_inference(dialect: Dialect) -> bool {
    matches!(
        dialect,
        Dialect::CommonLisp | Dialect::EmacsLisp | Dialect::Scheme | Dialect::Racket
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_emacs_lisp_scheme_and_racket_are_enabled() {
        for dialect in [
            Dialect::CommonLisp,
            Dialect::EmacsLisp,
            Dialect::Scheme,
            Dialect::Racket,
        ] {
            assert!(supports_type_inference(dialect), "{dialect:?}");
        }
        for dialect in [Dialect::Clojure, Dialect::Fennel, Dialect::Unknown] {
            assert!(!supports_type_inference(dialect), "{dialect:?}");
        }
    }
}
