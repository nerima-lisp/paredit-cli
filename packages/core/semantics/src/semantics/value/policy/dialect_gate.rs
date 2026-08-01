//! Which dialects the value layer runs for.

use paredit_core_syntax::dialect::Dialect;

/// Whether constant folding and propagation are enabled for `dialect`.
///
/// Common Lisp's `defconstant`, and Emacs Lisp's `defconst`/`defcustom`
/// initial value, are the two file-level constant sources
/// `service::propagation` recognizes; `policy::foldable_heads` names the
/// arithmetic and control forms each of the two dialects folds. Every other
/// dialect gets an empty table rather than one built on borrowed semantics.
#[must_use]
pub const fn supports_value_propagation(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::CommonLisp | Dialect::EmacsLisp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_lisp_and_emacs_lisp_are_enabled() {
        for dialect in [Dialect::CommonLisp, Dialect::EmacsLisp] {
            assert!(supports_value_propagation(dialect), "{dialect:?}");
        }
        for dialect in [
            Dialect::Scheme,
            Dialect::Racket,
            Dialect::Clojure,
            Dialect::Fennel,
            Dialect::Unknown,
        ] {
            assert!(!supports_value_propagation(dialect), "{dialect:?}");
        }
    }
}
